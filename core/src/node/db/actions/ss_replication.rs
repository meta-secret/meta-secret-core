use std::sync::Arc;

use tracing_attributes::instrument;

use crate::node::common::model::device::DeviceLinkBuilder;
use crate::node::common::model::user::UserDataMember;
use crate::node::common::model::vault::VaultData;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::SharedSecretRequest;

pub struct SSReplicationAction<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> SSReplicationAction<Repo> {
    #[instrument(skip_all)]
    pub async fn replicate(
        &self,
        request: SharedSecretRequest,
        vault: &VaultData,
    ) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        let ss_log_events = self.persistent_obj.find_object_events(request.ss_log).await?;

        commit_log.extend(ss_log_events);

        for UserDataMember(member) in vault.members() {
            if request.sender == member {
                continue;
            }

            let ss_event_id = {
                let device_link = DeviceLinkBuilder::builder()
                    .sender(member.device.id.clone())
                    .receiver(request.sender.device.id.clone())
                    .build()?;

                SharedSecretEventId {
                    vault_name: request.sender.vault_name.clone(),
                    device_link,
                }
            };

            let ss_split_events = {
                let ss_split_obj_desc = SharedSecretDescriptor::Split(ss_event_id.clone()).to_obj_desc();

                self.persistent_obj
                    .find_object_events(ObjectId::unit(ss_split_obj_desc))
                    .await?
            };
            commit_log.extend(ss_split_events);

            let ss_recover_events = {
                let ss_recover_obj_desc = SharedSecretDescriptor::Recover(ss_event_id).to_obj_desc();

                self.persistent_obj
                    .find_object_events(ObjectId::unit(ss_recover_obj_desc))
                    .await?
            };
            commit_log.extend(ss_recover_events);
        }

        Ok(commit_log)
    }
}
