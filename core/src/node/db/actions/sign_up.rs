use crate::models::{UserSignature, VaultDoc};
use crate::node::db::events::common::{LogEventKeyBasedRecord, MetaPassObject, ObjectCreator, PublicKeyRecord};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::events::vault_event::VaultObject;

pub struct SignUpAction {}

impl SignUpAction {
    pub fn accept(
        &self,
        sign_up_request: &KvLogEvent<UserSignature>,
        server_pk: &PublicKeyRecord,
    ) -> Vec<GenericKvLogEvent> {
        match &sign_up_request.key {
            KvKey::Empty { .. } => {
                panic!("SignUp error. Empty key");
            }
            KvKey::Key { obj_id, obj_desc } => {
                match obj_id.clone() {
                    ObjectId::Unit { .. } => match obj_desc.clone() {
                        ObjectDescriptor::Vault { vault_name } => {
                            let user_sig: UserSignature = sign_up_request.value.clone();

                            let genesis_event = {
                                let genesis_update = VaultObject::Genesis {
                                    event: KvLogEvent::genesis(&obj_desc, server_pk),
                                };
                                GenericKvLogEvent::Vault(genesis_update)
                            };

                            let sign_up_event = {
                                let vault = VaultDoc {
                                    vault_name: vault_name.clone(),
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

                            let meta_pass_unit_event = {
                                let event = KvLogEvent {
                                    key: KvKey::unit(&ObjectDescriptor::MetaPassword {
                                        vault_name: vault_name.clone(),
                                    }),
                                    value: (),
                                };
                                GenericKvLogEvent::MetaPass(MetaPassObject::Unit { event })
                            };

                            let meta_pass_genesis_event = {
                                let event = KvLogEvent::genesis(&ObjectDescriptor::MetaPassword { vault_name }, server_pk);
                                GenericKvLogEvent::MetaPass(MetaPassObject::Genesis { event })
                            };

                            vec![
                                generic_sign_up_request,
                                genesis_event,
                                sign_up_event,
                                meta_pass_unit_event,
                                meta_pass_genesis_event,
                            ]
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
    }
}

pub struct SignUpRequest {}

impl SignUpRequest {
    pub fn generic_request(&self, user_sig: &UserSignature) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(VaultObject::unit(user_sig))
    }
}
