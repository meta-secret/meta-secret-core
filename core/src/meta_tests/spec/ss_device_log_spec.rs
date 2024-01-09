use anyhow::Result;
use sha2::digest::typenum::assert_type;
use std::sync::Arc;

use crate::node::{
    common::model::user::UserData,
    db::{
        descriptors::{object_descriptor::ToObjectDescriptor, shared_secret::SharedSecretDescriptor},
        events::{
            common::{SSDeviceLogObject, VaultGenesisEvent, VaultUnitEvent},
            object_id::{Next, ObjectId, UnitId},
        },
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};

pub struct SSDeviceLogSpec<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user: UserData,
}

impl<Repo: KvLogEventRepo> SSDeviceLogSpec<Repo> {
    pub async fn check_initialization(&self) -> Result<()> {
        let ss_obj_desc = SharedSecretDescriptor::SSDeviceLog(self.user.device.id.clone()).to_obj_desc();
        
        let ss_unit_id = UnitId::unit(&ss_obj_desc);
        let ss_genesis_id = ss_unit_id.clone().next();

        let ss_gen_unit_obj = self
            .p_obj
            .repo
            .find_one(ObjectId::from(ss_unit_id))
            .await?
            .unwrap();

        let ss_unit_obj = SSDeviceLogObject::try_from(ss_gen_unit_obj)?;
        let VaultUnitEvent(unit_event) =ss_unit_obj.get_unit()?;
        assert_eq!(unit_event.value, self.user.vault_name());

        let genesis_event = self
            .p_obj
            .repo
            .find_one(ObjectId::from(ss_genesis_id))
            .await?
            .unwrap();


        let genesis_obj = SSDeviceLogObject::try_from(genesis_event)?;
        let VaultGenesisEvent(genesis_event) = genesis_obj.get_genesis()?;
        assert_eq!(genesis_event.value, self.user);

        Ok(())
    }
}
