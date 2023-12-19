use std::sync::Arc;

use tracing::debug;

use crate::node::common::model::user::UserData;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret::SharedSecretDescriptor;
use crate::node::db::events::common::SSDeviceLogObject;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, ObjectId, UnitId};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentSharedSecret<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl <Repo: KvLogEventRepo> PersistentSharedSecret<Repo> {

    pub async fn init(&self, user: UserData) -> anyhow::Result<()> {
        let user_id = user.user_id();
        let obj_desc = SharedSecretDescriptor::SSDeviceLog(user_id.device_id).to_obj_desc();
        let unit_id = UnitId::unit(&obj_desc);

        let maybe_unit_event = self.p_obj.repo
            .find_one(ObjectId::Unit(unit_id))
            .await?;

        if let Some(unit_event) = maybe_unit_event {
            debug!("SSDeviceLog already initialized: {:?}", unit_event);
            return Ok(());
        }

        //create new unit and genesis events
        let unit_key = KvKey::unit(obj_desc.clone());
        let unit_event = SSDeviceLogObject::Unit(KvLogEvent {
            key: unit_key.clone(),
            value: user_id.vault_name.clone(),
        });

        self.p_obj.repo.save(unit_event.to_generic()).await?;

        let genesis_key = unit_key.next();
        let genesis_event = SSDeviceLogObject::Genesis(KvLogEvent {
            key: genesis_key,
            value: user,
        });
        self.p_obj.repo.save(genesis_event.to_generic()).await?;

        Ok(())
    }
}