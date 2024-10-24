use std::sync::Arc;

use anyhow::bail;
use tracing::{debug, info};
use tracing_attributes::instrument;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserId, UserMembership};
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, Next, ObjectId, UnitId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault_event::VaultActionEvent;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentDeviceLog<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    pub async fn find_tail_id(&self, user_id: &UserId) -> anyhow::Result<Option<ObjectId>> {
        let obj_desc = VaultDescriptor::device_log(user_id.clone());
        self.p_obj.find_tail_id_by_obj_desc(obj_desc.clone()).await
    }
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    #[instrument(skip_all)]
    pub async fn save_accept_join_request_event(
        &self,
        member: UserDataMember,
        candidate: UserData,
    ) -> anyhow::Result<()> {
        info!("Accept join request");

        let member_user = member.user();
        self.init(member_user).await?;

        let key = self.get_device_log_key(&member_user).await?;
        let update = VaultActionEvent::UpdateMembership {
            sender: member,
            update: UserMembership::Member(UserDataMember { user_data: candidate }),
        };
        let join_request = DeviceLogObject::Action(KvLogEvent { key, value: update });

        self.p_obj.repo.save(join_request.to_generic()).await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn save_create_vault_request(&self, user: &UserData) -> anyhow::Result<()> {
        info!("Save event: CreateVault request");

        self.init(user).await?;

        let maybe_generic_event = self
            .p_obj
            .find_tail_event(VaultDescriptor::device_log(user.user_id()))
            .await?;

        if let Some(GenericKvLogEvent::DeviceLog(DeviceLogObject::Action(event))) = maybe_generic_event {
            if let VaultActionEvent::CreateVault(_) = event.value {
                info!("SignUp request already exists");
                return Ok(());
            }
        }

        let create_request = DeviceLogObject::Action(KvLogEvent {
            key: self.get_device_log_key(&user).await?,
            value: VaultActionEvent::CreateVault(user.clone()),
        });
        self.p_obj.repo.save(create_request.to_generic()).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_join_request(&self, user: &UserData) -> anyhow::Result<()> {
        self.init(user).await?;

        let join_request = DeviceLogObject::Action(KvLogEvent {
            key: self.get_device_log_key(&user).await?,
            value: VaultActionEvent::JoinClusterRequest {
                candidate: user.clone(),
            },
        });
        self.p_obj.repo.save(join_request.to_generic()).await?;

        Ok(())
    }

    async fn get_device_log_key(&self, user: &UserData) -> anyhow::Result<KvKey<ArtifactId>> {
        let obj_desc = VaultDescriptor::device_log(user.user_id());

        let free_id = self.p_obj.find_free_id_by_obj_desc(obj_desc.clone()).await?;

        let ObjectId::Artifact(free_artifact_id) = free_id else {
            bail!("Invalid free id: {:?}", free_id);
        };

        Ok(KvKey::artifact(obj_desc.clone(), free_artifact_id))
    }

    async fn init(&self, user: &UserData) -> anyhow::Result<()> {
        let user_id = user.user_id();
        let obj_desc = VaultDescriptor::device_log(user_id.clone());
        let unit_id = UnitId::unit(&obj_desc);

        let maybe_unit_event = self.p_obj.repo.find_one(ObjectId::Unit(unit_id)).await?;

        if let Some(unit_event) = maybe_unit_event {
            debug!("Device log already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit(obj_desc.clone());

        let unit_event = DeviceLogObject::Unit(VaultUnitEvent(KvLogEvent {
            key: unit_key.clone(),
            value: user_id.vault_name.clone(),
        }));

        self.p_obj.repo.save(unit_event.to_generic()).await?;

        let genesis_key = unit_key.next();
        let genesis_event = DeviceLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: genesis_key,
            value: user.clone(),
        }));
        self.p_obj.repo.save(genesis_event.to_generic()).await?;

        Ok(())
    }
}

#[cfg(test)]
pub mod spec {
    use std::sync::Arc;
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
    use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::Result;
    use log::info;
    use tracing_attributes::instrument;

    pub struct DeviceLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> DeviceLogSpec<Repo> {
        #[instrument(skip(self))]
        pub async fn check_initialization(&self) -> Result<()> {
            info!("Check initialization");

            let device_log_desc = VaultDescriptor::device_log(self.user.user_id());

            let unit_id = UnitId::unit(&device_log_desc);

            let unit_event_vault_name = self
                .p_obj
                .repo
                .find_one(ObjectId::from(unit_id.clone()))
                .await?
                .unwrap()
                .device_log()?
                .get_unit()?
                .vault_name();

            assert_eq!(unit_event_vault_name, self.user.vault_name());

            let genesis_id = unit_id.clone().next();
            let genesis_user = self
                .p_obj
                .repo
                .find_one(ObjectId::from(genesis_id))
                .await?
                .unwrap()
                .device_log()?
                .get_genesis()?
                .user();

            assert_eq!(genesis_user, self.user);

            Ok(())
        }

        #[instrument(skip(self))]
        pub async fn check_sign_up_request(&self) -> Result<()> {
            info!("check_sign_up_request");

            let device_log_desc = VaultDescriptor::device_log(self.user.user_id());

            let sign_up_request_id = UnitId::unit(&device_log_desc).next().next();

            let candidate = self
                .p_obj
                .repo
                .find_one(ObjectId::from(sign_up_request_id))
                .await?
                .unwrap()
                .device_log()?
                .get_action()?
                .get_create()?;
            assert_eq!(candidate.device, self.user.device);

            Ok(())
        }
    }
}