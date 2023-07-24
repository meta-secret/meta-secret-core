use crate::models::{UserSignature, VaultDoc};
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::models::{
    GenericKvLogEvent, KvKey, KvLogEvent, LogEventKeyBasedRecord, ObjectCreator, ObjectDescriptor, ObjectType,
    PublicKeyRecord, VaultObject,
};

pub struct SignUpAction {}

impl SignUpAction {
    pub fn accept(
        &self,
        sign_up_request: &KvLogEvent<UserSignature>,
        server_pk: &PublicKeyRecord,
    ) -> Vec<GenericKvLogEvent> {
        match sign_up_request.key.obj_id.clone() {
            ObjectId::Unit { .. } => match sign_up_request.key.object_type {
                ObjectType::VaultObj => {
                    let user_sig: UserSignature = sign_up_request.value.clone();

                    let genesis_event = {
                        let vault_name = user_sig.vault.name.clone();

                        let obj_desc = ObjectDescriptor::Vault { name: vault_name };
                        let genesis_update = VaultObject::Genesis {
                            event: KvLogEvent::genesis(&obj_desc, server_pk),
                        };
                        GenericKvLogEvent::Vault(genesis_update)
                    };

                    let sign_up_event = {
                        let vault = VaultDoc {
                            vault_name: user_sig.vault.name.clone(),
                            signatures: vec![user_sig],
                            pending_joins: vec![],
                            declined_joins: vec![],
                        };

                        let sign_up_event = KvLogEvent {
                            key: genesis_event.key().next(),
                            value: vault,
                        };
                        GenericKvLogEvent::Vault(VaultObject::SignUpUpdate { event: sign_up_event })
                    };

                    let generic_sign_up_request = GenericKvLogEvent::Vault(VaultObject::Unit {
                        event: sign_up_request.clone(),
                    });

                    vec![generic_sign_up_request, genesis_event, sign_up_event]
                }
                _ => {
                    panic!("Wrong object type")
                }
            },
            ObjectId::Genesis { .. } => {
                panic!("Invalid object id");
            }
            ObjectId::Regular { .. } => {
                panic!("Invalid object id");
            }
        }
    }
}

pub struct SignUpRequest {}

impl SignUpRequest {
    pub fn generic_request(&self, user_sig: &UserSignature) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(VaultObject::Unit {
            event: self.build_request(user_sig),
        })
    }

    pub fn build_request(&self, user_sig: &UserSignature) -> KvLogEvent<UserSignature> {
        let obj_desc = ObjectDescriptor::Vault {
            name: user_sig.vault.name.clone(),
        };

        KvLogEvent {
            key: KvKey::unit(&obj_desc),
            value: user_sig.clone(),
        }
    }
}
