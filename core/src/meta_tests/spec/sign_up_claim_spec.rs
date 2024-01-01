use std::sync::Arc;

use crate::node::{
    common::model::user::UserData,
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, shared_secret::SharedSecretDescriptor, vault::VaultDescriptor,
        },
        events::{
            object_id::{Next, ObjectId, UnitId},
            vault_event::{DeviceLogObject, VaultAction},
        },
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::Result;

use super::test_spec::TestSpec;
use async_trait::async_trait;

pub struct SignUpClaimSpec<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user: UserData,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> TestSpec for SignUpClaimSpec<Repo> {
    async fn check(&self) -> Result<()> {
        let device_log_desc = VaultDescriptor::device_log(self.user.user_id());

        let unit_id = UnitId::unit(&device_log_desc);
        let genesis_id = unit_id.clone().next();
        let request_id = genesis_id.clone().next();

        let _unit_event = self.p_obj.repo.find_one(ObjectId::from(unit_id)).await?.unwrap();
        let _genesis_event = self.p_obj.repo.find_one(ObjectId::from(genesis_id)).await?.unwrap();

        let generic_sign_up_request = self.p_obj.repo.find_one(ObjectId::from(request_id)).await?.unwrap();

        let sign_up_request = DeviceLogObject::try_from(generic_sign_up_request)?;

        let DeviceLogObject::Action(vault_action_event) = sign_up_request else {
            panic!("Invalid action event");
        };

        let vault_action = vault_action_event.value;
        let VaultAction::Create(candidate) = vault_action else {
            panic!("Invalid action: {:?}", vault_action);
        };

        assert_eq!(candidate.device, self.user.device);

        // check SSLog
        let ss_obj_desc = SharedSecretDescriptor::SSDeviceLog(self.user.device.id.clone()).to_obj_desc();
        let ss_unit_id = UnitId::unit(&ss_obj_desc);
        let ss_genesis_id = ss_unit_id.clone().next();

        let _ = self.p_obj.repo.find_one(ObjectId::from(ss_unit_id)).await?;
        let _ = self.p_obj.repo.find_one(ObjectId::from(ss_genesis_id)).await?;

        Ok(())
    }
}
