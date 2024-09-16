use std::sync::Arc;

use anyhow::bail;
use tracing::debug;
use tracing_attributes::instrument;

use crate::node::common::model::user::{UserData, UserDataMember, UserId, UserMembership};
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, Next, ObjectId, UnitId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault_event::{DeviceLogObject, VaultAction};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentDeviceLog<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentDeviceLog<Repo> {
    pub async fn find_tail_id(&self, user_id: &UserId) -> anyhow::Result<Option<ObjectId>> {
        todo!("Add validation steps that the user is a member of the vault");
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
        let member_user = member.user();
        self.init(member_user).await?;

        let key = self.get_device_log_artifact_key(&member_user).await?;
        let update = VaultAction::UpdateMembership {
            sender: member,
            update: UserMembership::Member(UserDataMember(candidate)),
        };
        let join_request = DeviceLogObject::Action(KvLogEvent { key, value: update });

        self.p_obj.repo.save(join_request.to_generic()).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_create_vault_request(&self, user: &UserData) -> anyhow::Result<()> {
        self.init(user).await?;

        let create_request = DeviceLogObject::Action(KvLogEvent {
            key: self.get_device_log_artifact_key(&user).await?,
            value: VaultAction::CreateVault(user.clone()),
        });
        self.p_obj.repo.save(create_request.to_generic()).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn save_join_request(&self, user: &UserData) -> anyhow::Result<()> {
        self.init(user).await?;

        let join_request = DeviceLogObject::Action(KvLogEvent {
            key: self.get_device_log_artifact_key(&user).await?,
            value: VaultAction::JoinClusterRequest{ candidate: user.clone()},
        });
        self.p_obj.repo.save(join_request.to_generic()).await?;

        Ok(())
    }

    async fn get_device_log_artifact_key(&self, user: &UserData) -> anyhow::Result<KvKey<ArtifactId>> {
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
