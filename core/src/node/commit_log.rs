use std::ops::Deref;
use std::rc::Rc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::crypto;

use crate::models::{AppCommandType, Base64EncodedText, KvKey, KvLogEvent, KvValueType, UserSignature, VaultDoc};

pub struct MetaDb {
    pub meta_store: MetaStore,
}

pub struct DbLog {
    pub events: Vec<KvLogEvent>,
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

fn accept(request: KvLogEvent, meta_db: MetaDb) -> KvLogEvent {
    let mut maybe_error = None;
    if request.cmd_type != AppCommandType::JoinCluster {
        maybe_error = Some("Not allowed cmd_type");
    }

    if request.val_type != KvValueType::UserSignature {
        maybe_error = Some("Not allowed val_type");
    }

    if let None = request.value {
        maybe_error = Some("Empty user signature");
    }

    if let None = meta_db.meta_store.vault {
        maybe_error = Some("Db doesn't have a vault");
    }

    if let Some(err_msg) = maybe_error {
        return KvLogEvent {
            key: request.key,
            cmd_type: AppCommandType::Update,
            val_type: KvValueType::Error,
            value: Some(serde_json::from_str(err_msg).unwrap()),
        };
    }

    let user_sig_and_vault = (request.value, meta_db.meta_store.vault);
    if let (Some(user_sig_val), Some(db_vault)) = user_sig_and_vault {
        let user_sig: UserSignature = serde_json::from_str(user_sig_val.as_str()).unwrap();

        let mut vault = db_vault.clone();
        let members = &mut vault.signatures;
        members.push(user_sig);

        //create a log event
        KvLogEvent {
            key: Box::from(KvKey { store: request.key.store, id: crypto::utils::rand_uuid_b64_url_enc() }),
            cmd_type: AppCommandType::Update,
            val_type: KvValueType::Vault,
            value: Some(serde_json::to_string(&vault).unwrap()),
        }
    } else {
        todo!("Impossible!")
    }
}

fn transform(commit_log: Rc<Vec<KvLogEvent>>) -> Result<MetaDb, LogCommandError> {
    let mut meta_db = MetaDb {
        meta_store: MetaStore {
            server_pk: None,
            vault: None,
        },
    };

    for (index, event) in commit_log.iter().enumerate() {
        match index {
            0 => {
                if event.cmd_type == AppCommandType::Genesis {
                    let server_pk_str = event.value.as_ref().unwrap().as_str();
                    let server_pk: Base64EncodedText = serde_json::from_str(server_pk_str).unwrap();
                    meta_db.meta_store.server_pk = Some(server_pk);
                } else {
                    return Err(LogCommandError::IllegalDbState { err_msg: "Missing genesis event".to_string() });
                }
            }
            _ => {
                panic!("yay")
            }
        }
    }

    Ok(meta_db)
}

fn generate_genesis_event(value: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: Box::from(KvKey {
            store: "commit_log".to_string(),
            id: crypto::utils::rand_uuid_b64_url_enc(),
        }),
        cmd_type: AppCommandType::Genesis,
        val_type: KvValueType::DsaPublicKey,
        value: Some(serde_json::to_string(&value).unwrap()),
    }
}

#[cfg(test)]
pub mod test {
    use std::rc::Rc;
    use crate::crypto;

    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::models::{DeviceInfo, KvKey, KvLogEvent, KvValueType, UserCredentials, VaultDoc};
    use crate::node::commit_log::{accept, AppCommandType, generate_genesis_event, LogCommandError, MetaDb, MetaStore, transform};

    //genesis:
    // - generate keys on server
    // - server sends commit log with genesis event to a client
    // - client reads commit log and adds genesis event into meta_db
    #[test]
    fn genesis_event() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();

        let genesis_event = generate_genesis_event(&server_km.dsa.public_key());

        let commit_log = vec![genesis_event];
        let meta_db = transform(Rc::new(commit_log))?;
        assert_eq!(meta_db.meta_store.server_pk, Some(server_km.dsa.public_key()));
        Ok(())
    }

    #[test]
    fn test() {
        let server_km = KeyManager::generate();
        let genesis_event = generate_genesis_event(&server_km.dsa.public_key());

        let vault_name = "test".to_string();

        //server provides to client a commit log that contains only one record (genesis), which contains server's pk
        let mut command_log = vec![genesis_event];

        let a_s_box = KeyManager::generate_security_box(vault_name.clone());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);
        let a_creds = UserCredentials::new(a_s_box, a_user_sig.clone());

        let sign_up_event = KvLogEvent {
            key: Box::from(KvKey {
                store: "commit_log".to_string(),
                id: crypto::utils::rand_uuid_b64_url_enc(),
            }),
            cmd_type: AppCommandType::SignUp,
            val_type: KvValueType::UserSignature,
            value: Some(serde_json::to_string(&a_user_sig).unwrap()),
        };

        command_log.push(sign_up_event);

        let b_s_box = KeyManager::generate_security_box(vault_name.clone());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let join_command = KvLogEvent {
            key: Box::from(KvKey {
                store: "commit_log".to_string(),
                id: crypto::utils::rand_uuid_b64_url_enc(),
            }),
            cmd_type: AppCommandType::JoinCluster,
            val_type: KvValueType::UserSignature,
            value: Some(serde_json::to_string(&b_user_sig).unwrap()),
        };

        let meta_db = MetaDb {
            meta_store: MetaStore {
                server_pk: None,
                vault: Some(VaultDoc {
                    vault_name,
                    signatures: vec![a_user_sig],
                    pending_joins: vec![],
                    declined_joins: vec![],
                }),
            },
        };

        let join_event = accept(join_command, meta_db);
        command_log.push(join_event);

        for command in command_log {
            println!("{}", serde_json::to_string(&command).unwrap());
            println!("")
        }
    }
}