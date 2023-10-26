use crate::node::common::model::device::DeviceCredentials;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{ObjIdExtractor, UnitEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCredentialsObject {
    pub event: KvLogEvent<UnitId, DeviceCredentials>
}

impl ObjIdExtractor for DeviceCredentialsObject {
    fn obj_id(&self) -> ObjectId {
        ObjectId::from(self.event.key.clone())
    }
}

impl UnitEvent<DeviceCredentials> for DeviceCredentialsObject {
    fn unit(value: DeviceCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::DeviceCredsIndex),
            value,
        };

        DeviceCredentialsObject { event }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTailObject {
    pub event: KvLogEvent<UnitId, DbTail>
}

impl ObjIdExtractor for DbTailObject {
    fn obj_id(&self) -> ObjectId {
        ObjectId::from(self.event.key.clone())
    }
}

impl UnitEvent<DbTail> for DbTailObject {
    fn unit(value: DbTail) -> Self {
        let key = KvKey::unit(ObjectDescriptor::DbTail);
        let event = KvLogEvent { key, value };
        Self { event }
    }
}
