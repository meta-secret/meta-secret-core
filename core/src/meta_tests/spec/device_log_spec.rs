use std::sync::Arc;

use anyhow::Result;

use crate::node::{
    db::{
        descriptors::vault_descriptor::VaultDescriptor,
        events::object_id::{Next, ObjectId, UnitId},
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};
use crate::node::common::model::user::common::UserData;

pub struct DeviceLogSpec<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user: UserData,
}

impl<Repo: KvLogEventRepo> DeviceLogSpec<Repo> {
    pub async fn check_initialization(&self) -> Result<()> {
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

    pub async fn check_sign_up_request(&self) -> Result<()> {
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
