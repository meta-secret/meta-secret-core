use crate::models::{UserSignature, VaultDoc};
use crate::node::db::commit_log::generate_next;
use crate::node::db::models::{AppOperation, AppOperationType, KvKey, KvLogEvent, KvValueType};

pub fn join_cluster_request(prev_key: &KvKey, user_sig: &UserSignature) -> KvLogEvent {
    KvLogEvent {
        key: generate_next(prev_key),
        cmd_type: AppOperationType::Request(AppOperation::JoinCluster),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_value(user_sig).unwrap(),
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

    let user_sig: UserSignature = serde_json::from_value(request.value.clone()).unwrap();

    let mut new_vault = vault.clone();
    new_vault.signatures.push(user_sig);

    KvLogEvent {
        key: generate_next(&request.key),
        cmd_type: AppOperationType::Update(AppOperation::JoinCluster),
        val_type: KvValueType::Vault,
        value: serde_json::to_value(&new_vault).unwrap(),
    }
}
