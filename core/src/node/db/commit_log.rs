use std::collections::HashSet;
use std::rc::Rc;

use crate::crypto;
use crate::crypto::utils::to_id;
use crate::models::{Base64EncodedText, UserSignature, VaultDoc};

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
    pub vaults_index: HashSet<String>,
    pub vault: Option<VaultDoc>,
}

#[derive(thiserror::Error, Debug)]
pub enum LogCommandError {
    #[error("Illegal message format: {err_msg:?}")]
    IllegalCommandFormat { err_msg: String },
    #[error("Illegal database state: {err_msg:?}")]
    IllegalDbState { err_msg: String },
    #[error(transparent)]
    JsonParseError {
        #[from]
        source: serde_json::Error,
    },
}

pub mod store_names {
    pub const GENESIS: &str = "genesis";
    pub const COMMIT_LOG: &str = "commit_log";
    pub const VAULTS_IDX: &str = "vaults_idx";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum AppOperation {
    #[serde(rename = "Genesis")]
    Genesis,
    #[serde(rename = "SignUp")]
    SignUp,
    #[serde(rename = "JoinCluster")]
    JoinCluster,
    #[serde(rename = "VaultsIndex")]
    VaultsIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum AppOperationType {
    #[serde(rename = "Request")]
    Request(AppOperation),
    #[serde(rename = "Update")]
    Update(AppOperation),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KvLogEvent {
    #[serde(rename = "key")]
    pub key: KvKey,
    #[serde(rename = "cmdType")]
    pub cmd_type: AppOperationType,
    #[serde(rename = "valType")]
    pub val_type: KvValueType,
    #[serde(rename = "value")]
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum KvValueType {
    #[serde(rename = "DsaPublicKey")]
    DsaPublicKey,
    #[serde(rename = "UserSignature")]
    UserSignature,
    #[serde(rename = "Vault")]
    Vault,
    #[serde(rename = "String")]
    String,
    #[serde(rename = "Base64Text")]
    Base64Text,
    #[serde(rename = "Error")]
    Error,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvKey {
    pub id: String,
    pub prev_id: String,
    pub store: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<String>,
}

pub fn accept_event_sign_up_request(event: &KvLogEvent) -> Vec<KvLogEvent> {
    if event.cmd_type != AppOperationType::Request(AppOperation::SignUp) {
        panic!("Invalid request");
    }

    let user_sig: UserSignature = serde_json::from_str(event.value.clone().as_str()).unwrap();
    accept_sign_up_request(&event.key, &user_sig)
}

pub fn accept_sign_up_request(prev: &KvKey, user_sig: &UserSignature) -> Vec<KvLogEvent> {
    let vault_id = to_id(user_sig.vault_name.clone()).base64_text;
    let vaults_index = vaults_index_event(prev.id.clone(), vault_id.clone());

    let vault = VaultDoc {
        vault_name: user_sig.vault_name.clone(),
        signatures: vec![user_sig.clone()],
        pending_joins: vec![],
        declined_joins: vec![],
    };

    let sign_up_event = KvLogEvent {
        key: generate_commit_log_key(prev.id.clone(), Some(vault_id)),
        cmd_type: AppOperationType::Update(AppOperation::SignUp),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_string(&vault).unwrap(),
    };

    vec![sign_up_event, vaults_index]
}

pub fn generate_genesis_key() -> KvKey {
    generate_key(store_names::GENESIS, "-1".to_string(), None)
}

pub fn generate_commit_log_key(prev_id: String, vault_id: Option<String>) -> KvKey {
    generate_key(store_names::COMMIT_LOG, prev_id, vault_id)
}

pub fn generate_key(store_name: &str, prev_id: String, vault_id: Option<String>) -> KvKey {
    KvKey {
        store: store_name.to_string(),
        id: crypto::utils::rand_uuid_b64_url_enc().base64_text,
        prev_id,
        vault_id,
    }
}


fn generate_next(prev_key: &KvKey) -> KvKey {
    KvKey {
        id: crypto::utils::rand_uuid_b64_url_enc().base64_text,
        prev_id: prev_key.id.clone(),
        store: "commit_log".to_string(),
        vault_id: prev_key.vault_id.clone(),
    }
}


pub fn accept_join_request(request: &KvLogEvent, vault: &VaultDoc) -> KvLogEvent {
    let mut maybe_error = None;
    if request.cmd_type != AppOperationType::Request(AppOperation::JoinCluster) {
        maybe_error = Some("Not allowed cmd_type");
    }

    if request.val_type != KvValueType::UserSignature {
        maybe_error = Some("Not allowed val_type");
    }

    if let Some(err_msg) = maybe_error {
        return KvLogEvent {
            key: generate_next(&request.key),
            cmd_type: AppOperationType::Update(AppOperation::JoinCluster),
            val_type: KvValueType::Error,
            value: serde_json::from_str(err_msg).unwrap(),
        };
    }

    let user_sig_str = request.value.as_str();
    let user_sig: UserSignature = serde_json::from_str(user_sig_str).unwrap();

    let mut new_vault = vault.clone();
    new_vault.signatures.push(user_sig);

    KvLogEvent {
        key: generate_next(&request.key),
        cmd_type: AppOperationType::Update(AppOperation::JoinCluster),
        val_type: KvValueType::Vault,
        value: serde_json::to_string(&new_vault).unwrap(),
    }
}

pub fn transform(commit_log: Rc<Vec<KvLogEvent>>) -> Result<MetaDb, LogCommandError> {
    let mut meta_db = MetaDb {
        meta_store: MetaStore {
            server_pk: None,
            vaults_index: HashSet::new(),
            vault: None,
        },
    };

    for (index, event) in commit_log.iter().enumerate() {
        let mut meta_store = &mut meta_db.meta_store;

        match index {
            0 => {
                if event.cmd_type == AppOperationType::Update(AppOperation::Genesis) {
                    let server_pk_str = event.value.as_str();
                    let server_pk: Base64EncodedText = serde_json::from_str(server_pk_str).unwrap();
                    meta_db.meta_store.server_pk = Some(server_pk);
                } else {
                    return Err(LogCommandError::IllegalDbState {
                        err_msg: "Missing genesis event".to_string(),
                    });
                }
            }
            _ => match event.cmd_type {
                AppOperationType::Request(_op) => {
                    println!("Skip requests");
                }

                AppOperationType::Update(op) => match op {
                    AppOperation::Genesis => {}
                    AppOperation::SignUp => {
                        let vault: VaultDoc = serde_json::from_str(event.value.as_str()).unwrap();
                        meta_store.vault = Some(vault);
                    }
                    AppOperation::JoinCluster => {
                        let vault: VaultDoc = serde_json::from_str(event.value.as_str()).unwrap();
                        meta_store.vault = Some(vault);
                    }
                    AppOperation::VaultsIndex => {
                        let vault_id: String = serde_json::from_str(event.value.as_str()).unwrap();
                        meta_store.vaults_index.insert(vault_id);
                    }
                },
            },
        }
    }

    Ok(meta_db)
}

pub fn generate_genesis_event(value: &Base64EncodedText) -> KvLogEvent {
    KvLogEvent {
        key: generate_genesis_key(),
        cmd_type: AppOperationType::Update(AppOperation::Genesis),
        val_type: KvValueType::DsaPublicKey,
        value: serde_json::to_string(&value).unwrap(),
    }
}

pub fn vaults_index_event(prev_id: String, vault_id: String) -> KvLogEvent {
    KvLogEvent {
        key: generate_commit_log_key(prev_id, Some(vault_id.clone())),
        cmd_type: AppOperationType::Update(AppOperation::VaultsIndex),
        val_type: KvValueType::String,
        value: serde_json::to_string(vault_id.as_str()).unwrap(),
    }
}

pub fn sign_up_request(prev_key: &KvKey, user_sig: &UserSignature) -> KvLogEvent {
    let vault_id = to_id(user_sig.vault_name.clone());

    KvLogEvent {
        key: generate_commit_log_key(prev_key.prev_id.clone(), Some(vault_id.base64_text)),
        cmd_type: AppOperationType::Request(AppOperation::SignUp),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_string(user_sig).unwrap(),
    }
}

pub fn join_cluster_request(prev_key: &KvKey, user_sig: &UserSignature) -> KvLogEvent {
    KvLogEvent {
        key: generate_next(prev_key),
        cmd_type: AppOperationType::Request(AppOperation::JoinCluster),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_string(user_sig).unwrap(),
    }
}

#[cfg(test)]
pub mod test {
    use std::collections::HashSet;
    use std::rc::Rc;

    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::crypto::utils::to_id;
    use crate::models::{DeviceInfo, VaultDoc};
    use crate::node::db::commit_log::{accept_join_request, accept_sign_up_request, generate_genesis_event, join_cluster_request, KvLogEvent, LogCommandError, sign_up_request, transform, vaults_index_event};

    #[test]
    fn test_vaults_index() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();

        let genesis_event: KvLogEvent = generate_genesis_event(&server_km.dsa.public_key());
        // vaultName -> sha256 -> uuid
        let vault_id = to_id("test_vault".to_string());
        let vaults_index_event = vaults_index_event(genesis_event.key.id.clone(), vault_id.base64_text.clone());

        let commit_log = vec![genesis_event, vaults_index_event];

        let meta_db = transform(Rc::new(commit_log))?;

        let mut expected = HashSet::new();
        expected.insert(vault_id.base64_text);

        assert_eq!(expected, meta_db.meta_store.vaults_index);

        Ok(())
    }

    //genesis:
    // - generate keys on server
    // - server sends commit log with genesis event to a client
    // - client reads commit log and adds genesis event into meta_db
    #[test]
    fn test_genesis_event() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();

        let genesis_event = generate_genesis_event(&server_km.dsa.public_key());

        let commit_log = vec![genesis_event];
        let meta_db = transform(Rc::new(commit_log))?;
        assert_eq!(meta_db.meta_store.server_pk, Some(server_km.dsa.public_key()));
        Ok(())
    }

    #[test]
    fn test_sign_up() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();
        let genesis_event = generate_genesis_event(&server_km.dsa.public_key());

        let vault_name = "test".to_string();

        let a_s_box = KeyManager::generate_security_box(vault_name.clone());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let sign_up_event = sign_up_request(&genesis_event.key, &a_user_sig);

        let sing_up_accept = accept_sign_up_request(&sign_up_event.key, &a_user_sig);

        let mut commit_log = vec![genesis_event, sign_up_event];
        commit_log.extend(sing_up_accept);

        let meta_db = transform(Rc::new(commit_log))?;

        let vault = VaultDoc {
            vault_name,
            signatures: vec![a_user_sig],
            pending_joins: vec![],
            declined_joins: vec![],
        };

        assert_eq!(vault, meta_db.meta_store.vault.unwrap());

        Ok(())
    }

    #[test]
    fn test_join_cluster() -> Result<(), LogCommandError> {
        let server_km = KeyManager::generate();
        let genesis_event = generate_genesis_event(&server_km.dsa.public_key());

        let vault_name = "test".to_string();

        let a_s_box = KeyManager::generate_security_box(vault_name.clone());
        let a_device = DeviceInfo::new("a".to_string(), "a".to_string());
        let a_user_sig = a_s_box.get_user_sig(&a_device);

        let sign_up_event = sign_up_request(&genesis_event.key, &a_user_sig);

        let sing_up_accept = accept_sign_up_request(&sign_up_event.key, &a_user_sig);

        let b_s_box = KeyManager::generate_security_box(vault_name.clone());
        let b_device = DeviceInfo::new("b".to_string(), "b".to_string());
        let b_user_sig = b_s_box.get_user_sig(&b_device);

        let join_request = join_cluster_request(&sing_up_accept[1].key, &b_user_sig);

        let vault = VaultDoc {
            vault_name,
            signatures: vec![a_user_sig.clone()],
            pending_joins: vec![],
            declined_joins: vec![],
        };
        let join_cluster_event = accept_join_request(&join_request, &vault);

        let mut commit_log = vec![genesis_event, sign_up_event];
        commit_log.extend(sing_up_accept);
        commit_log.push(join_cluster_event);

        println!("{}", serde_json::to_string_pretty(&commit_log).unwrap());

        let meta_db = transform(Rc::new(commit_log))?;

        println!("meta db: {}", serde_json::to_string_pretty(&meta_db).unwrap());

        let expected_sigs = vec![a_user_sig, b_user_sig];
        assert_eq!(expected_sigs, meta_db.meta_store.vault.unwrap().signatures);

        Ok(())
    }
}
