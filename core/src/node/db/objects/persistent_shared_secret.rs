use std::sync::Arc;


use anyhow::{bail, Ok};
use tracing::debug;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{MetaPasswordId, SecretDistributionData};
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, ObjectId, UnitId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::shared_secret_event::{SSDeviceLogObject, SSLedgerObject};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentSharedSecret<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {
    pub async fn find_device_tail_id(&self, device_id: &DeviceId) -> anyhow::Result<Option<ObjectId>> {
        let obj_desc = SharedSecretDescriptor::SSDeviceLog(device_id.clone()).to_obj_desc();
        self.p_obj.find_tail_id_by_obj_desc(obj_desc).await
    }

    pub async fn get_ledger(&self, vault_name: VaultName) -> anyhow::Result<SSLedgerObject> {
        let obj_desc = SharedSecretDescriptor::SSLedger(vault_name).to_obj_desc();
        let maybe_ledger_event = self.p_obj.find_tail_event(obj_desc).await?;

        let Some(ledger_event) = maybe_ledger_event else {
            bail!("Invalid SSLedger object event");
        };

        let ledger_obj = SSLedgerObject::try_from(ledger_event)?;
        Ok(ledger_obj)
    }

    pub async fn get_local_share(&self, pass_id: MetaPasswordId) -> anyhow::Result<GenericKvLogEvent> {
        let obj_desc = SharedSecretDescriptor::LocalShare(pass_id).to_obj_desc();
        let unit_id = UnitId::unit(&obj_desc);
        let Some(local_share_generic_event) = self.p_obj.repo.find_one(ObjectId::Unit(unit_id)).await? else {
            bail!("Invalid Local share object event");
        };

        Ok(local_share_generic_event)
    }

    pub async fn get_local_share_distribution_data(
        &self,
        pass_id: MetaPasswordId,
    ) -> anyhow::Result<SecretDistributionData> {
        let ss_obj = self.get_local_share(pass_id).await?.shared_secret()?;

        ss_obj.to_local_share()
    }

    pub async fn init(&self, user: UserData) -> anyhow::Result<()> {
        self.init_device_log(user.clone()).await?;
        self.init_ss_ledger(user).await?;

        Ok(())
    }

    pub async fn init_device_log(&self, user: UserData) -> anyhow::Result<()> {
        let user_id = user.user_id();
        let obj_desc = SharedSecretDescriptor::SSDeviceLog(user_id.device_id).to_obj_desc();
        let unit_id = UnitId::unit(&obj_desc);

        let maybe_unit_event = self.p_obj.repo.find_one(ObjectId::Unit(unit_id)).await?;

        if let Some(unit_event) = maybe_unit_event {
            debug!("SSDeviceLog already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit(obj_desc.clone());
        let unit_event = SSDeviceLogObject::Unit(VaultUnitEvent(KvLogEvent {
            key: unit_key.clone(),
            value: user_id.vault_name.clone(),
        }));

        self.p_obj.repo.save(unit_event.to_generic()).await?;

        let genesis_key = unit_key.next();
        let genesis_event = SSDeviceLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: genesis_key,
            value: user.clone(),
        }));
        self.p_obj.repo.save(genesis_event.to_generic()).await?;

        Ok(())
    }

    async fn init_ss_ledger(&self, user: UserData) -> anyhow::Result<()> {
        let ledger_desc = SharedSecretDescriptor::SSLedger(user.vault_name()).to_obj_desc();
        let unit_id = UnitId::unit(&ledger_desc);

        let maybe_unit_event = self.p_obj.repo.find_one(ObjectId::Unit(unit_id)).await?;
        if let Some(unit_event) = maybe_unit_event {
            debug!("SSLedger already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit(ledger_desc.clone());
        let unit_event = SSLedgerObject::Unit(VaultUnitEvent(KvLogEvent {
            key: unit_key.clone(),
            value: user.vault_name(),
        }));

        self.p_obj.repo.save(unit_event.to_generic()).await?;

        let genesis_key = unit_key.next();
        let genesis_event = SSLedgerObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: genesis_key,
            value: user,
        }));
        self.p_obj.repo.save(genesis_event.to_generic()).await?;

        Ok(())
    }
}
