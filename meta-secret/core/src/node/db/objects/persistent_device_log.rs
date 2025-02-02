use std::sync::Arc;

use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsider, UserId, UserMembership,
};
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{
    ArtifactId, Next, ObjectId, UnitId, VaultGenesisEvent, VaultUnitEvent,
};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, CreateVaultEvent, JoinClusterEvent, VaultActionEvent, VaultActionRequestEvent, 
    VaultActionUpdateEvent
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{bail, Result};
use tracing::{debug, info};
use tracing_attributes::instrument;

pub struct PersistentDeviceLog<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    pub async fn find_tail_id(&self, user_id: &UserId) -> Result<Option<ObjectId>> {
        let obj_desc = DeviceLogDescriptor::from(user_id.clone());
        self.p_obj.find_tail_id_by_obj_desc(obj_desc.clone()).await
    }
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    #[instrument(skip_all)]
    pub async fn save_accept_join_request_event(
        &self,
        member: UserDataMember,
        candidate: UserDataOutsider,
    ) -> Result<()> {
        info!("Accept join request");

        let member_user = member.user();
        self.init(member_user).await?;

        let free_key = self.get_device_log_key(member_user).await?;
        let update = VaultActionUpdateEvent::UpdateMembership {
            request: ???,
            sender: member,
            update: UserMembership::Member(UserDataMember::from(candidate)),
        };

        let join_request = DeviceLogObject::Action(KvLogEvent {
            key: free_key,
            value: VaultActionEvent::Update(update),
        });

        self.p_obj.repo.save(join_request).await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn save_create_vault_request(&self, user: &UserData) -> Result<()> {
        info!("Save event: CreateVault request");

        self.init(user).await?;

        let maybe_device_log_event = self
            .p_obj
            .find_tail_event(DeviceLogDescriptor::from(user.user_id()))
            .await?;

        if let Some(DeviceLogObject::Action(event)) = maybe_device_log_event {
            if let VaultActionEvent::Update(VaultActionUpdateEvent::CreateVault { .. }) =
                event.value
            {
                info!("SignUp request already exists");
                return Ok(());
            }
        }

        let create_request = {
            let upd = VaultActionUpdateEvent::CreateVault(CreateVaultEvent {
                owner: user.clone(),
            });
            DeviceLogObject::Action(KvLogEvent {
                key: self.get_device_log_key(&user).await?,
                value: VaultActionEvent::Update(upd),
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

            DeviceLogObject::Action(KvLogEvent {
                key: self.get_device_log_key(sender.user()).await?,
                value: VaultActionEvent::Update(add_meta_pass),
            })
        };

        self.p_obj.repo.save(meta_pass_request).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_join_request(&self, user: &UserData) -> Result<()> {
        self.init(user).await?;

        let request = VaultActionRequestEvent::JoinCluster(JoinClusterEvent::from(user.clone()));
        let join_request = DeviceLogObject::Action(KvLogEvent {
            key: self.get_device_log_key(user).await?,
            value: VaultActionEvent::Request(request)
        });
        self.p_obj.repo.save(join_request).await?;

        Ok(())
    }

    async fn get_device_log_key(&self, user: &UserData) -> Result<KvKey<ArtifactId>> {
        let obj_desc = DeviceLogDescriptor::from(user.user_id());

        let free_id = self
            .p_obj
            .find_free_id_by_obj_desc(obj_desc.clone())
            .await?;

        let ObjectId::Artifact(free_artifact_id) = free_id else {
            bail!("Invalid free id: {:?}", free_id);
        };

        Ok(KvKey::artifact(obj_desc.to_obj_desc(), free_artifact_id))
    }

    async fn init(&self, user: &UserData) -> Result<()> {
        let user_id = user.user_id();
        let obj_desc = DeviceLogDescriptor::from(user_id.clone());
        let unit_id = UnitId::from(obj_desc.clone());

        let maybe_unit_event = self.p_obj.repo.find_one(ObjectId::Unit(unit_id)).await?;

        if let Some(unit_event) = maybe_unit_event {
            debug!("Device log already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit_from(obj_desc.clone());

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
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
    use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
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
        pub async fn check_initialization(&self) -> Result<()> {
            info!("Check initialization");

            let device_log_desc = DeviceLogDescriptor::from(self.user.user_id());

            let unit_id = UnitId::from(device_log_desc);

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

            let device_log_desc = DeviceLogDescriptor::from(self.user.user_id());

            let sign_up_request_id = UnitId::from(device_log_desc).next().next();

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
