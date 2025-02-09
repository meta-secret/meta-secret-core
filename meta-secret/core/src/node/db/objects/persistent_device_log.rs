use std::sync::Arc;

use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsider, UserId, UserMembership,
};
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, CreateVaultEvent, JoinClusterEvent, VaultActionEvent, VaultActionInitEvent,
    VaultActionRequestEvent, VaultActionUpdateEvent,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;
use tracing::info;
use tracing_attributes::instrument;

pub struct PersistentDeviceLog<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    pub async fn find_tail_id(&self, user_id: &UserId) -> Result<Option<ArtifactId>> {
        let obj_desc = DeviceLogDescriptor::from(user_id.clone());
        self.p_obj.find_tail_id_by_obj_desc(obj_desc.clone()).await
    }
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    #[instrument(skip_all)]
    pub async fn save_accept_join_request_event(
        &self,
        join_request: JoinClusterEvent,
        member: UserDataMember,
        candidate: UserDataOutsider,
    ) -> Result<()> {
        info!("Accept join request");

        let member_user = member.user();

        let free_key = self.get_device_log_key(member_user).await?;
        let update = VaultActionUpdateEvent::UpdateMembership {
            request: join_request,
            sender: member,
            update: UserMembership::Member(UserDataMember::from(candidate)),
        };

        let join_request = DeviceLogObject(KvLogEvent {
            key: free_key,
            value: VaultActionEvent::Update(update),
        });

        self.p_obj.repo.save(join_request).await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn save_create_vault_request(&self, user: &UserData) -> Result<()> {
        info!("Save event: CreateVault request");

        let create_request = {
            let create_action = VaultActionInitEvent::CreateVault(CreateVaultEvent {
                owner: user.clone(),
            });
            let upd = VaultActionEvent::Init(create_action);
            DeviceLogObject(KvLogEvent {
                key: self.get_device_log_key(user).await?,
                value: upd,
            })
        };
        self.p_obj.repo.save(create_request).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_add_meta_pass_request(
        &self,
        sender: UserDataMember,
        meta_pass_id: MetaPasswordId,
    ) -> Result<()> {
        let meta_pass_request = {
            let add_meta_pass = VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent {
                sender: sender.clone(),
                meta_pass_id,
            });

            DeviceLogObject(KvLogEvent {
                key: self.get_device_log_key(sender.user()).await?,
                value: VaultActionEvent::Update(add_meta_pass),
            })
        };

        self.p_obj.repo.save(meta_pass_request).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_join_request(&self, user: &UserData) -> Result<()> {
        let request = VaultActionRequestEvent::JoinCluster(JoinClusterEvent::from(user.clone()));
        let join_request = DeviceLogObject(KvLogEvent {
            key: self.get_device_log_key(user).await?,
            value: VaultActionEvent::Request(request),
        });
        self.p_obj.repo.save(join_request).await?;

        Ok(())
    }

    async fn get_device_log_key(&self, user: &UserData) -> Result<KvKey> {
        let obj_desc = DeviceLogDescriptor::from(user.user_id());

        let free_id = self
            .p_obj
            .find_free_id_by_obj_desc(obj_desc.clone())
            .await?;

        Ok(KvKey::artifact(obj_desc.to_obj_desc(), free_id))
    }
}

#[cfg(test)]
pub mod spec {
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
    use crate::node::db::events::object_id::{ArtifactId, Next};
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::Result;
    use log::info;
    use std::sync::Arc;
    use tracing_attributes::instrument;

    pub struct DeviceLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> DeviceLogSpec<Repo> {
        #[instrument(skip(self))]
        pub async fn check_sign_up_request(&self) -> Result<()> {
            info!("check_sign_up_request");

            let device_log_desc = DeviceLogDescriptor::from(self.user.user_id());

            //TODO add verification

            Ok(())
        }
    }
}
