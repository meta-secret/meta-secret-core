use anyhow::Result;
use std::sync::Arc;

use crate::node::{
    common::model::user::UserData,
    db::{
        descriptors::{object_descriptor::ToObjectDescriptor, shared_secret_descriptor::SharedSecretDescriptor},
        events::object_id::{Next, ObjectId, UnitId},
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};

pub struct SSDeviceLogSpec<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub client_user: UserData,
}

impl<Repo: KvLogEventRepo> SSDeviceLogSpec<Repo> {
    pub async fn check_initialization(&self) -> Result<()> {
        let ss_obj_desc = SharedSecretDescriptor::SSDeviceLog(self.client_user.device.id.clone()).to_obj_desc();

        let ss_unit_id = UnitId::unit(&ss_obj_desc);
        let ss_genesis_id = ss_unit_id.clone().next();

        let maybe_unit_event = self.p_obj.repo.find_one(ObjectId::from(ss_unit_id)).await?;

        if let Some(unit_event) = maybe_unit_event {
            let vault_name = unit_event.ss_device_log()?.get_unit()?.vault_name();
            assert_eq!(vault_name, self.client_user.vault_name());
        } else {
            panic!("SSDevice, unit event not found");
        }

        let maybe_genesis_event = self.p_obj.repo.find_one(ObjectId::from(ss_genesis_id)).await?;

        if let Some(genesis_event) = maybe_genesis_event {
            let user = genesis_event.ss_device_log()?.get_genesis()?.user();
            assert_eq!(user, self.client_user);
        } else {
            panic!("SSDevice, genesis event not found");
        }

        Ok(())
    }
}
