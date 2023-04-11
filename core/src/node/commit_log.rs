use std::rc::Rc;
use crate::crypto;

use crate::models::{AppCommandType, Base64EncodedText, KvKey, KvLogEvent, KvValueType, UserSignature, VaultDoc};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaDb {
    pub meta_store: MetaStore,
}

pub struct DbLog {
    pub events: Vec<KvLogEvent>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
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

fn accept_sign_up_request(vault: &VaultDoc) -> KvLogEvent {
    KvLogEvent {
        key: Box::from(generate_key()),
        cmd_type: AppCommandType::Update,
        val_type: KvValueType::Vault,
        value: Some(serde_json::to_string(&vault).unwrap()),
    }
}

fn generate_key() -> KvKey {
    KvKey {
        store: "commit_log".to_string(),
        id: crypto::utils::rand_uuid_b64_url_enc()
    }
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
                match event.cmd_type {
                    AppCommandType::Update => {
                        match event.value.as_ref() {
                            None => {

                            }
                            Some(vault_str) => {
                                let vault: VaultDoc = serde_json::from_str(vault_str.as_str()).unwrap();
                                meta_db.meta_store.vault = Some(vault);
                            }
                        }
                    }
                    AppCommandType::Genesis => {
                        todo!("Not implemented yet");
                    }
                    AppCommandType::SignUp => {
                        println!("found sign up request");
                    }
                    AppCommandType::JoinCluster => {
                        todo!("Not implemented yet");
                    }
                }
            }
        }
    }

    Ok(meta_db)
}

fn generate_genesis_event(value: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: Box::from(generate_key()),
        cmd_type: AppCommandType::Genesis,
        val_type: KvValueType::DsaPublicKey,
        value: Some(serde_json::to_string(&value).unwrap()),
    }
}

fn sign_up_event(user_sig: &UserSignature) -> KvLogEvent {
    KvLogEvent {
        key: Box::from(generate_key()),
        cmd_type: AppCommandType::SignUp,
        val_type: KvValueType::UserSignature,
        value: Some(serde_json::to_string(user_sig).unwrap()),
    }
}

fn join_cluster_event(user_sig: &UserSignature) -> KvLogEvent {
    KvLogEvent {
        key: Box::from(generate_key()),
        cmd_type: AppCommandType::JoinCluster,
        val_type: KvValueType::UserSignature,
        value: Some(serde_json::to_string(user_sig).unwrap()),
    }
}

#[cfg(test)]
pub mod test {
    use std::rc::Rc;

    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::models::{DeviceInfo, VaultDoc};
    use crate::node::commit_log::{accept, accept_sign_up_request, generate_genesis_event, join_cluster_event, LogCommandError, sign_up_event, transform};

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
    fn sign_up() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();
        let genesis_event = generate_genesis_event(&server_km.dsa.public_key());

        let vault_name = "test".to_string();

        let a_s_box = KeyManager::generate_security_box(vault_name.clone());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let sign_up_event = sign_up_event(&a_user_sig);

        let vault = VaultDoc {
            vault_name,
            signatures: vec![a_user_sig],
            pending_joins: vec![],
            declined_joins: vec![],
        };
        let sing_up_accept = accept_sign_up_request(&vault);

        let commit_log = vec![genesis_event, sign_up_event, sing_up_accept];
        let meta_db = transform(Rc::new(commit_log))?;

        assert_eq!(vault, meta_db.meta_store.vault.unwrap());

        Ok(())
    }
}
