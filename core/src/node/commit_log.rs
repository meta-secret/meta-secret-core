use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::crypto;

use crate::models::{Base64EncodedText, UserSignature, VaultDoc};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub store: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub key: Key,
    pub cmd_type: CommandType,
    pub val_type: ValueType,
    pub value: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CommandType {
    /// The event that contains data and just updates the database
    /// (essentially this is an event that happens as a result of an action that was executed,
    /// see Genesis, SingUp and other commands).
    Update,

    /// Commands
    Genesis,
    SignUp,
    JoinCluster,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ValueType {
    DsaPublicKey,
    UserSignature,
    Vault,
    Error,
}

pub struct MetaDb {
    pub meta_store: MetaStore,
}

pub struct DbLog {
    pub events: Vec<LogEvent>,
}

pub struct MetaStore {
    pub server_pk: Option<Base64EncodedText>,
    pub vault: Option<VaultDoc>,
}

#[derive(thiserror::Error, Debug)]
pub enum LogCommandError {
    #[error("Illegal message format: {err_msg:?}")]
    IllegalCommandFormat {
        err_msg: String
    },
    #[error("Illegal database state: {err_msg:?}")]
    IllegalDbState {
        err_msg: String
    },
    #[error(transparent)]
    JsonParseError {
        #[from]
        source: serde_json::Error,
    },
}

fn accept(request: LogEvent, meta_db: MetaDb) -> LogEvent {
    let mut maybe_error = None;
    if request.cmd_type != CommandType::JoinCluster {
        maybe_error = Some("Not allowed cmd_type");
    }

    if request.val_type != ValueType::UserSignature {
        maybe_error = Some("Not allowed val_type");
    }

    if let None = request.value {
        maybe_error = Some("Empty user signature");
    }

    if let None = meta_db.meta_store.vault {
        maybe_error = Some("Db doesn't have a vault");
    }

    if let Some(err_msg) = maybe_error {
        return LogEvent {
            key: request.key,
            cmd_type: CommandType::Update,
            val_type: ValueType::Error,
            value: Some(serde_json::from_str(err_msg).unwrap()),
        };
    }

    let user_sig_and_vault = (request.value, meta_db.meta_store.vault);
    if let (Some(user_sig_val), Some(db_vault)) = user_sig_and_vault {
        let user_sig: UserSignature = serde_json::from_value(user_sig_val).unwrap();

        let mut vault = db_vault.clone();
        let members = &mut vault.signatures;
        members.push(user_sig);

        //create a log event
        LogEvent {
            key: Key { store: request.key.store, id: crypto::utils::rand_uuid_b64_url_enc() },
            cmd_type: CommandType::Update,
            val_type: ValueType::Vault,
            value: Some(serde_json::to_value(vault).unwrap()),
        }
    } else {
        todo!("Impossible!")
    }
}

#[cfg(test)]
pub mod test {
    use ed25519_dalek::Signer;
    use serde_json::Value;
    use crate::crypto;

    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::models::{DeviceInfo, UserCredentials, VaultDoc};
    use crate::node::commit_log::{accept, CommandType, DbLog, Key, LogEvent, MetaDb, MetaStore, ValueType};

    #[test]
    fn test() {
        let vault_name = "test".to_string();

        let server_km = KeyManager::generate();

        let genesis_event = LogEvent {
            key: Key {
                store: "commit_log".to_string(),
                id: crypto::utils::rand_uuid_b64_url_enc(),
            },
            cmd_type: CommandType::Genesis,
            val_type: ValueType::DsaPublicKey,
            value: Some(serde_json::to_value(server_km.dsa.public_key()).unwrap()),
        };


        //server provides to client a commit log that contains only one record (genesis), which contains server's pk
        let mut command_log = vec![genesis_event];

        let a_s_box = KeyManager::generate_security_box(vault_name.clone());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);
        let a_creds = UserCredentials::new(a_s_box, a_user_sig.clone());

        let sign_up_event = LogEvent {
            key: Key {
                store: "commit_log".to_string(),
                id: crypto::utils::rand_uuid_b64_url_enc(),
            },
            cmd_type: CommandType::SignUp,
            val_type: ValueType::UserSignature,
            value: Some(serde_json::to_value(a_user_sig.clone()).unwrap()),
        };

        command_log.push(sign_up_event);

        let b_s_box = KeyManager::generate_security_box(vault_name.clone());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let join_command = LogEvent {
            key: Key {
                store: "commit_log".to_string(),
                id: crypto::utils::rand_uuid_b64_url_enc()
            },
            cmd_type: CommandType::JoinCluster,
            val_type: ValueType::UserSignature,
            value: Some(serde_json::to_value(b_user_sig).unwrap()),
        };

        let meta_db = MetaDb {
            meta_store: MetaStore {
                server_pk: None,
                vault: Some(VaultDoc{
                    vault_name,
                    signatures: vec![a_user_sig],
                    pending_joins: vec![],
                    declined_joins: vec![],
                })
            },
        };

        let join_event = accept(join_command, meta_db);
        command_log.push(join_event);

        for command in command_log {
            println!("{}", serde_json::to_string(&command).unwrap());
            println!("")
        }
    }

    // sign_up (server side)
    //fn sign_up(server_km: KeyManager, vault: VaultDoc) -> LogEvent {
    // sign with a server key
    // save vault in the db
    //let vault_json_str = serde_json::to_string(&vault).unwrap();
    //}
}