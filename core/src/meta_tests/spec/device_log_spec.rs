use std::sync::Arc;

use anyhow::Result;

use crate::node::{
    common::model::user::UserData,
    db::{
        descriptors::vault::VaultDescriptor,
        events::{
            object_id::{Next, ObjectId, UnitId},
            vault_event::{DeviceLogObject, VaultAction},
        },
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};

pub struct DeviceLogSpec<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user: UserData,
}

impl<Repo: KvLogEventRepo> DeviceLogSpec<Repo> {
    pub async fn check_initialization(&self) -> Result<()> {
        let device_log_desc = VaultDescriptor::device_log(self.user.user_id());

        let unit_id = UnitId::unit(&device_log_desc);

        let generic_unit_event = self
            .p_obj
            .repo
            .find_one(ObjectId::from(unit_id.clone()))
            .await?
            .unwrap();

        if let DeviceLogObject::Unit(unit_event) = DeviceLogObject::try_from(generic_unit_event)? {
            assert_eq!(unit_event.value, self.user.vault_name());
        } else {
            panic!("Invalid unit event");
        }

        let genesis_id = unit_id.clone().next();
        let generic_genesis_event = self.p_obj.repo.find_one(ObjectId::from(genesis_id)).await?.unwrap();

        if let DeviceLogObject::Genesis(genesis_event) = DeviceLogObject::try_from(generic_genesis_event)? {
            assert_eq!(genesis_event.value, self.user);
        } else {
            panic!("Invalid genesis event");
        }

        Ok(())
    }

    pub async fn check_sign_up_request(&self) -> Result<()> {
        let device_log_desc = VaultDescriptor::device_log(self.user.user_id());

        let sign_up_request_id = UnitId::unit(&device_log_desc).next().next();

        let generic_sign_up_request = self
            .p_obj
            .repo
            .find_one(ObjectId::from(sign_up_request_id))
            .await?
            .unwrap();

        let sign_up_request = DeviceLogObject::try_from(generic_sign_up_request)?;

        let DeviceLogObject::Action(vault_action_event) = sign_up_request else {
            panic!("Invalid action event");
        };

        let vault_action = vault_action_event.value;
        if let VaultAction::Create(candidate) = vault_action {
            assert_eq!(candidate.device, self.user.device);
        } else {
            panic!("Invalid action: {:?}", vault_action);
        }

        Ok(())
    }
}
