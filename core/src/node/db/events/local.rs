use crate::node::common::model::device::DeviceCredentials;
use crate::node::common::model::user::UserCredentials;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{ObjIdExtractor, UnitEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{ObjectId, UnitId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialsObject {
    Device {
        event: KvLogEvent<UnitId, DeviceCredentials>
    },
    User {
        event: KvLogEvent<UnitId, UserCredentials>
    }
}

impl ObjIdExtractor for CredentialsObject {
    fn obj_id(&self) -> ObjectId {
        CredentialsObject::unit_id()
    }
}

impl UnitEvent<DeviceCredentials> for CredentialsObject {

    fn unit(value: DeviceCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::CredsIndex),
            value,
        };

        CredentialsObject::Device { event }
    }
}

impl UnitEvent<UserCredentials> for CredentialsObject {

    fn unit(value: UserCredentials) -> Self {
        let event = KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::CredsIndex),
            value,
        };

        CredentialsObject::User { event }
    }
}

impl CredentialsObject {
    pub fn unit_id() -> ObjectId {
        ObjectId::unit(ObjectDescriptor::CredsIndex)
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
