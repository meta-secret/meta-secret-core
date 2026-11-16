use crate::node::common::model::user::common::{UserData, UserDataMember, UserId};
use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, CreateVaultEvent, JoinClusterEvent, RemoveMemberEvent,
    RemoveMemberRequestEvent, UpdateMembershipEvent, VaultActionEvent, VaultActionInitEvent,
    VaultActionRequestEvent, VaultActionUpdateEvent,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;
use derive_more::From;
use std::sync::Arc;

#[derive(From)]
pub struct PersistentDeviceLog<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    pub async fn save_vault_action(
        &self,
        sender: &UserData,
        action: VaultActionEvent,
    ) -> Result<()> {
        self.append_owned(sender, action, None).await?;
        Ok(())
    }
    pub async fn find_tail_id(&self, user_id: &UserId) -> Result<Option<ArtifactId>> {
        Ok(self
            .transport_slots(
                user_id,
                ArtifactId::from(DeviceLogDescriptor::from(user_id.clone())),
            )
            .await?
            .last()
            .map(|slot| slot.obj_id()))
    }
    pub async fn latest_join_request(&self, user: &UserData) -> Result<Option<JoinClusterEvent>> {
        let events = self
            .transport_slots(
                &user.user_id(),
                ArtifactId::from(DeviceLogDescriptor::from(user.user_id())),
            )
            .await?;
        Ok(events
            .into_iter()
            .rev()
            .filter_map(|slot| slot.business())
            .find_map(|object| {
                let VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(request)) =
                    object.0.value
                else {
                    return None;
                };
                (request.candidate == *user).then_some(request)
            }))
    }
    pub async fn save_updated_membership_event(&self, update: UpdateMembershipEvent) -> Result<()> {
        let sender = update.sender.user_data.clone();
        self.save_vault_action(
            &sender,
            VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(update)),
        )
        .await
    }
    pub async fn save_create_vault_request(&self, user: &UserData) -> Result<()> {
        self.save_vault_action(
            user,
            VaultActionEvent::Init(VaultActionInitEvent::CreateVault(CreateVaultEvent {
                owner: UserDataMember::from(user.clone()),
            })),
        )
        .await
    }
    pub async fn save_add_meta_pass_request(&self, event: AddMetaPassEvent) -> Result<()> {
        let sender = event.sender.user_data.clone();
        self.save_vault_action(
            &sender,
            VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(event)),
        )
        .await
    }
    pub async fn save_join_request(&self, user: &UserData) -> Result<()> {
        self.save_vault_action(
            user,
            VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(
                JoinClusterEvent::from(user.clone()),
            )),
        )
        .await
    }
    pub async fn save_remove_member_request(
        &self,
        request: RemoveMemberRequestEvent,
    ) -> Result<()> {
        let sender = request.sender.user_data.clone();
        self.save_vault_action(
            &sender,
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)),
        )
        .await
    }
    pub async fn save_remove_member_update(&self, update: RemoveMemberEvent) -> Result<()> {
        let sender = update.sender.user_data.clone();
        self.save_vault_action(
            &sender,
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
        )
        .await
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod spec {
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::Result;
    use std::sync::Arc;
    use tracing::info;
    use tracing_attributes::instrument;

    pub struct DeviceLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> DeviceLogSpec<Repo> {
        #[instrument(skip(self))]
        pub async fn check_sign_up_request(&self) -> Result<()> {
            info!("check_sign_up_request");

            let _device_log_desc = DeviceLogDescriptor::from(self.user.user_id());

            //TODO add verification

            Ok(())
        }
    }
}
