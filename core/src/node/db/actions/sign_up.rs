use crate::node::common::model::device::DeviceData;
use crate::node::common::model::user::{UserDataCandidate, UserDataMember, UserMembership};
use crate::node::common::model::vault::VaultData;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, UnitId};
use crate::node::db::events::vault_event::{VaultLogObject, VaultObject, VaultStatusObject};

pub struct SignUpAction {}

impl SignUpAction {
    pub fn accept(&self, candidate: &UserDataCandidate, server: DeviceData) -> Vec<GenericKvLogEvent> {
        let mut commit_log = vec![];

        let vault_name = candidate.user_data.vault_name.clone();

        let vault_log_events = {
            let vault_log_obj_desc = VaultDescriptor::vault_log(vault_name.clone());
            let unit_event = GenericKvLogEvent::VaultLog(VaultLogObject::Unit {
                event: KvLogEvent {
                    key: KvKey::unit(vault_log_obj_desc.clone()),
                    value: vault_name.clone(),
                },
            });

            let genesis_event = GenericKvLogEvent::VaultLog(VaultLogObject::Genesis {
                event: KvLogEvent {
                    key: KvKey::genesis(vault_log_obj_desc),
                    value: candidate.clone(),
                },
            });

            vec![unit_event, genesis_event]
        };
        commit_log.extend(vault_log_events);

        let vault_events = {
            let vault_obj_desc = VaultDescriptor::vault(vault_name.clone());
            let unit_event = GenericKvLogEvent::Vault(VaultObject::Unit {
                event: KvLogEvent {
                    key: KvKey::unit(vault_obj_desc.clone()),
                    value: vault_name.clone(),
                }
            });

            let genesis_event = GenericKvLogEvent::Vault(VaultObject::Genesis {
                event: KvLogEvent {
                    key: KvKey::genesis(vault_obj_desc),
                    value: server,
                }
            });

            let vault_event = {
                let vault_data = {
                    let mut vault = VaultData::from(vault_name.clone());
                    let membership = UserMembership::Member(UserDataMember {
                        user_data: candidate.user_data.clone(),
                    });
                    vault.update_membership(membership);
                    vault
                };

                let vault_id = UnitId::vault_unit(vault_name.clone())
                    .next()
                    .next();

                let sign_up_event = KvLogEvent {
                    key: KvKey::artifact(vault_obj_desc.clone(), vault_id),
                    value: vault_data,
                };
                GenericKvLogEvent::Vault(VaultObject::Vault {
                    event: sign_up_event
                })
            };

            vec![unit_event, genesis_event, vault_event]
        };
        commit_log.extend(vault_events);

        let vault_status_events = {
            let vault_status_desc = ObjectDescriptor::Vault(VaultDescriptor::VaultStatus {
                vault_name: vault_name.clone(),
                device_id: candidate.user_data.device.id.clone(),
            });

            let unit_event = GenericKvLogEvent::VaultStatus(VaultStatusObject::Unit {
                event: KvLogEvent {
                    key: KvKey::unit(vault_status_desc.clone()),
                    value: vault_name.clone(),
                }
            });

            let genesis_event = GenericKvLogEvent::VaultStatus(VaultStatusObject::Genesis {
                event: KvLogEvent {
                    key: KvKey::genesis(vault_status_desc.clone()),
                    value: candidate.user_data.clone(),
                }
            });

            let status_event = {
                let status_event_id = UnitId::unit(vault_status_desc.clone())
                    .next()
                    .next();

                GenericKvLogEvent::VaultStatus(VaultStatusObject::Status {
                    event: KvLogEvent {
                        key: KvKey {
                            obj_id: status_event_id,
                            obj_desc: vault_status_desc,
                        },
                        value: UserMembership::Member(UserDataMember {
                            user_data: candidate.user_data.clone(),
                        })
                    },
                });
            };

            vec![unit_event, genesis_event, status_event]
        };
        commit_log.extend(vault_status_events);

        commit_log
    }
}
