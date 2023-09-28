use crate::models::{UserSignature, VaultDoc};
use crate::node::db::events::common::PublicKeyRecord;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "vault_obj")]
pub enum VaultObject {
    /// SingUp request
    Unit {
        event: KvLogEvent<UserSignature>,
    },
    Genesis {
        event: KvLogEvent<PublicKeyRecord>,
    },

    SignUpUpdate {
        event: KvLogEvent<VaultDoc>,
    },

    JoinUpdate {
        event: KvLogEvent<VaultDoc>,
    },

    JoinRequest {
        event: KvLogEvent<UserSignature>,
    },
}

impl VaultObject {
    pub fn key(&self) -> &KvKey {
        match self {
            VaultObject::Unit { event } => &event.key,
            VaultObject::Genesis { event } => &event.key,
            VaultObject::SignUpUpdate { event } => &event.key,
            VaultObject::JoinUpdate { event } => &event.key,
            VaultObject::JoinRequest { event } => &event.key,
        }
    }
}

impl VaultObject {
    pub fn unit(user_sig: &UserSignature) -> Self {
        VaultObject::Unit {
            event: KvLogEvent::vault_unit(user_sig),
        }
    }
}
