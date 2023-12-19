use std::sync::Arc;

use tracing::debug;
use tracing_attributes::instrument;

use crate::node::common::model::user::{UserData, UserDataMember, UserMembership};
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
use crate::node::db::events::vault_event::{DeviceLogObject, VaultAction};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentDeviceLog<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {

    #[instrument(skip_all)]
    pub async fn accept_join_cluster_request(&self, user: UserData) -> anyhow::Result<()> {
        self.init(&user).await?;

        let obj_desc = {
            let user_id = user.user_id();
            VaultDescriptor::device_log(user_id)
        };

        let free_id = self.p_obj
            .find_free_id_by_obj_desc(obj_desc.clone())
            .await?;

        let ObjectId::Artifact(free_artifact_id) = free_id else {
            return Ok(());
        };

        //join request!!!!
        let artifact_key = KvKey::artifact(obj_desc.clone(), free_artifact_id);
        let join_request = DeviceLogObject::Action(KvLogEvent {
            key: artifact_key,
            value: VaultAction::UpdateMembership {
                sender: UserDataMember(user.clone()),
                update: UserMembership::Member(UserDataMember(user)),
            },
        });
        self.p_obj.repo
            .save(join_request.to_generic())
            .await?;

        Ok(())
    }

    async fn init(&self, user: &UserData) -> anyhow::Result<()> {
        let user_id = user.user_id();
        let obj_desc = VaultDescriptor::device_log(user_id.clone());
        let unit_id = UnitId::unit(&obj_desc);

        let maybe_unit_event = self.p_obj.repo
            .find_one(ObjectId::Unit(unit_id))
            .await?;

        if let Some(unit_event) = maybe_unit_event {
            debug!("Device log already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit(obj_desc.clone());
        let unit_event = DeviceLogObject::Unit(KvLogEvent {
            key: unit_key.clone(),
            value: user_id.vault_name.clone(),
        });

        self.p_obj.repo.save(unit_event.to_generic()).await?;

        let genesis_key = unit_key.next();
        let genesis_event = DeviceLogObject::Genesis(KvLogEvent {
            key: genesis_key,
            value: user.clone(),
        });
        self.p_obj.repo.save(genesis_event.to_generic()).await?;

        Ok(())
    }
}
