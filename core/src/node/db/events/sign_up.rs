use crate::models::{Base64EncodedText, UserSignature, VaultDoc};
use crate::node::db::models::{
    AppOperation, AppOperationType, KeyIdGen, KvKey, KvLogEvent, KvValueType, ObjectCreator, ObjectDescriptor,
    ObjectType,
};
use crate::node::server::persistent_object_repo::ObjectFormation;

pub trait SignUpAction: ObjectFormation {
    fn sign_up_accept(&self, sign_up_request: &KvLogEvent, server_pk: Base64EncodedText) -> Vec<KvLogEvent> {
        if sign_up_request.cmd_type != AppOperationType::Request(AppOperation::SignUp) {
            panic!("Invalid request");
        }

        let user_sig: UserSignature = serde_json::from_value(sign_up_request.value.clone()).unwrap();
        let vault_name = user_sig.vault.name.clone();

        let vault = VaultDoc {
            vault_name: user_sig.vault.name.clone(),
            signatures: vec![user_sig],
            pending_joins: vec![],
            declined_joins: vec![],
        };

        let obj_desc = ObjectDescriptor::vault(vault_name.as_str());
        let vault_formation_event = self.formation_event(&obj_desc, &server_pk);

        let expected_sign_request_id = vault_formation_event.key.key_id.next();
        let actual_sign_up_request_id = sign_up_request.key.key_id.clone();
        if actual_sign_up_request_id != expected_sign_request_id {
            panic!(
                "Invalid request: invalid id. expected_sign_request_id: {:?}, actual_sign_up_request_id: {:?}",
                expected_sign_request_id, actual_sign_up_request_id
            );
        }

        let sign_up_event = KvLogEvent {
            key: KvKey {
                object_type: ObjectType::Vault,
                key_id: expected_sign_request_id.next(),
            },
            cmd_type: AppOperationType::Update(AppOperation::SignUp),
            val_type: KvValueType::Vault,
            value: serde_json::to_value(&vault).unwrap(),
        };

        vec![vault_formation_event, sign_up_request.clone(), sign_up_event]
    }
}

pub trait SignUpRequest: ObjectFormation {
    fn sign_up_request(&self, user_sig: &UserSignature) -> KvLogEvent {
        let obj_desc = ObjectDescriptor::vault(user_sig.vault.name.as_str());
        let genesis_key = KvKey::formation(&obj_desc);

        KvLogEvent {
            key: genesis_key.next(),
            cmd_type: AppOperationType::Request(AppOperation::SignUp),
            val_type: KvValueType::UserSignature,
            value: serde_json::to_value(user_sig).unwrap(),
        }
    }
}
