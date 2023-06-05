use crate::models::{Base64EncodedText, UserSignature, VaultDoc};
use crate::node::db::events::persistent_vault::create_vault_formation_event_on_server;
use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKey, KvKeyId, KvLogEvent, KvValueType, ObjectType,
};

pub fn accept_event_sign_up_request(sign_up_request: KvLogEvent, server_pk: Base64EncodedText) -> Vec<KvLogEvent> {
    if sign_up_request.cmd_type != AppOperationType::Request(AppOperation::SignUp) {
        panic!("Invalid request");
    }

    accept_sign_up_request(sign_up_request, server_pk)
}

pub fn accept_sign_up_request(sign_up_request: KvLogEvent, server_pk: Base64EncodedText) -> Vec<KvLogEvent> {
    let user_sig: UserSignature = serde_json::from_value(sign_up_request.value.clone()).unwrap();
    let vault_name = user_sig.vault_name.clone();

    let vault = VaultDoc {
        vault_name: user_sig.vault_name.clone(),
        signatures: vec![user_sig],
        pending_joins: vec![],
        declined_joins: vec![],
    };

    let vault_formation_event = create_vault_formation_event_on_server(vault_name.as_str(), &server_pk);

    let expected_sign_request_id = vault_formation_event.key.id.next();
    let actual_sign_up_request_id = sign_up_request.key.id.clone();
    if actual_sign_up_request_id != expected_sign_request_id {
        panic!("Invalid request: invalid id. expected_sign_request_id: {:?}, actual_sign_up_request_id: {:?}", expected_sign_request_id, actual_sign_up_request_id);
    }

    let vault_id = sign_up_request.key.vault_id.clone().unwrap();
    let sign_up_event = KvLogEvent {
        key: KvKey {
            object_type: ObjectType::Vault,
            id: expected_sign_request_id.next(),
            vault_id: Some(vault_id),
        },
        cmd_type: AppOperationType::Update(AppOperation::SignUp),
        val_type: KvValueType::Vault,
        value: serde_json::to_value(&vault).unwrap(),
    };

    vec![vault_formation_event, sign_up_request, sign_up_event]
}

pub fn sign_up_request(user_sig: &UserSignature) -> KvLogEvent {
    let id = KvKeyId::object_foundation(user_sig.vault_name.as_str(), ObjectType::Vault);

    let sign_up_key = KvKey {
        id: id.next(),
        object_type: ObjectType::Vault,
        vault_id: Some(id.key_id),
    };

    KvLogEvent {
        key: sign_up_key,
        cmd_type: AppOperationType::Request(AppOperation::SignUp),
        val_type: KvValueType::UserSignature,
        value: serde_json::to_value(user_sig).unwrap(),
    }
}
