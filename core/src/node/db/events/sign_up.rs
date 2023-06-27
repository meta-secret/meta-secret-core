use crate::models::{UserSignature, VaultDoc};
use crate::node::db::models::{
    GenericKvLogEvent, KeyIdGen, KvKey, KvLogEvent, KvLogEventRequest, KvLogEventUpdate, ObjectCreator,
    ObjectDescriptor, ObjectType, PublicKeyRecord,
};
use crate::node::server::persistent_object_repo::ObjectFormation;

pub trait SignUpAction: ObjectFormation {
    fn sign_up_accept(
        &self,
        sign_up_request: &KvLogEvent<UserSignature>,
        server_pk: &PublicKeyRecord,
    ) -> Vec<GenericKvLogEvent> {
        let user_sig: UserSignature = sign_up_request.value.clone();
        let vault_name = user_sig.vault.name.clone();

        let vault = VaultDoc {
            vault_name: user_sig.vault.name.clone(),
            signatures: vec![user_sig],
            pending_joins: vec![],
            declined_joins: vec![],
        };

        let obj_desc = ObjectDescriptor::Vault { name: vault_name };
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
                object_type: ObjectType::VaultObj,
                key_id: expected_sign_request_id.next(),
            },
            value: vault,
        };

        let genesis_update = KvLogEventUpdate::Genesis {
            event: vault_formation_event,
        };
        let vault_formation_event = GenericKvLogEvent::Update(genesis_update);

        let sign_up_request = GenericKvLogEvent::Request(KvLogEventRequest::SignUp {
            event: sign_up_request.clone(),
        });
        let sign_up_event = GenericKvLogEvent::Update(KvLogEventUpdate::SignUp { event: sign_up_event });

        vec![vault_formation_event, sign_up_request, sign_up_event]
    }
}

pub trait SignUpRequest: ObjectFormation {
    fn sign_up_generic_request(&self, user_sig: &UserSignature) -> GenericKvLogEvent {
        GenericKvLogEvent::Request(KvLogEventRequest::SignUp {
            event: self.sign_up_request(user_sig),
        })
    }

    fn sign_up_request(&self, user_sig: &UserSignature) -> KvLogEvent<UserSignature> {
        let obj_desc = ObjectDescriptor::Vault { name: user_sig.vault.name.clone() };
        let genesis_key = KvKey::formation(&obj_desc);

        KvLogEvent {
            key: genesis_key.next(),
            value: user_sig.clone(),
        }
    }
}
