use crate::models::{MetaVault, SecretDistributionDocData, UserCredentials};
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};

/// Local events (persistent objects which lives only in the local environment) which must not be synchronized
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "local_evt_obj")]
pub enum KvLogEventLocal {
    MetaVault {
        event: Box<KvLogEvent<MetaVault>>,
    },
    UserCredentials {
        event: Box<KvLogEvent<UserCredentials>>,
    },
    DbTail {
        event: Box<KvLogEvent<DbTail>>,
    },

    LocalSecretShare {
        event: KvLogEvent<SecretDistributionDocData>,
    },
}

impl KvLogEventLocal {
    pub fn key(&self) -> &KvKey {
        match self {
            KvLogEventLocal::DbTail { event } => &event.key,
            KvLogEventLocal::MetaVault { event } => &event.key,
            KvLogEventLocal::UserCredentials { event } => &event.key,
            KvLogEventLocal::LocalSecretShare { event } => &event.key,
        }
    }
}
