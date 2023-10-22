use crate::node::common::model::user::UserDataCandidate;
use crate::node::common::model::vault::VaultData;
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultObject {
    /// SingUp request
    Unit {
        event: KvLogEvent<UserDataCandidate>,
    },
    Genesis {
        event: KvLogEvent<PublicKeyRecord>,
    },
    JoinUpdate {
        event: KvLogEvent<VaultData>,
    },
    JoinRequest {
        event: KvLogEvent<UserDataCandidate>,
    },
}

impl VaultObject {
    pub fn key(&self) -> &KvKey {
        match self {
            VaultObject::Unit { event } => &event.key,
            VaultObject::Genesis { event } => &event.key,
            VaultObject::JoinUpdate { event } => &event.key,
            VaultObject::JoinRequest { event } => &event.key,
        }
    }
}

impl VaultObject {
    pub fn unit(user_sig: &UserDataCandidate) -> Self {
        VaultObject::Unit {
            event: KvLogEvent::vault_unit(user_sig),
        }
    }
}
