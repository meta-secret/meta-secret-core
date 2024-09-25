use tracing_attributes::instrument;
use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserId, UserMembership};
use crate::node::common::model::vault::VaultData;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, UnitId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault_event::{VaultLogObject, VaultMembershipObject, VaultObject};

pub struct SignUpAction {}

impl SignUpAction {
    #[instrument(skip_all)]
    pub fn accept(&self, candidate: UserData, server: DeviceData) -> Vec<GenericKvLogEvent> {
        let mut commit_log = vec![];

        let vault_name = candidate.vault_name.clone();

        let vault_log_events = {
            let vault_log_obj_desc = VaultDescriptor::vault_log(vault_name.clone());
            let unit_event = VaultLogObject::Unit(VaultUnitEvent(KvLogEvent {
                key: KvKey::unit(vault_log_obj_desc.clone()),
                value: vault_name.clone(),
            }))
            .to_generic();

            let genesis_event = VaultLogObject::Genesis(VaultGenesisEvent(KvLogEvent {
                key: KvKey::genesis(vault_log_obj_desc),
                value: candidate.clone(),
            }))
            .to_generic();

            vec![unit_event, genesis_event]
        };
        commit_log.extend(vault_log_events);

        let vault_events = {
            let vault_obj_desc = VaultDescriptor::vault(vault_name.clone());
            let unit_event = VaultObject::Unit(VaultUnitEvent(KvLogEvent {
                key: KvKey::unit(vault_obj_desc.clone()),
                value: vault_name.clone(),
            }))
            .to_generic();

            let genesis_event = VaultObject::Genesis(KvLogEvent {
                key: KvKey::genesis(vault_obj_desc.clone()),
                value: server,
            })
            .to_generic();

            let vault_event = {
                let vault_data = {
                    let mut vault = VaultData::from(vault_name.clone());
                    let membership = UserMembership::Member(UserDataMember(candidate.clone()));
                    vault.update_membership(membership);
                    vault
                };

                let vault_id = UnitId::vault_unit(vault_name.clone()).next().next();

                let sign_up_event = KvLogEvent {
                    key: KvKey::artifact(vault_obj_desc.clone(), vault_id),
                    value: vault_data,
                };
                VaultObject::Vault(sign_up_event).to_generic()
            };

            vec![unit_event, genesis_event, vault_event]
        };
        commit_log.extend(vault_events);

        let vault_status_events = {
            let user_id = UserId {
                vault_name: vault_name.clone(),
                device_id: candidate.device.id.clone(),
            };
            let vault_status_desc = VaultDescriptor::VaultMembership(user_id).to_obj_desc();

            let unit_event = VaultMembershipObject::Unit(VaultUnitEvent(KvLogEvent {
                key: KvKey::unit(vault_status_desc.clone()),
                value: vault_name.clone(),
            }))
            .to_generic();

            let genesis_event = VaultMembershipObject::Genesis(VaultGenesisEvent(KvLogEvent {
                key: KvKey::genesis(vault_status_desc.clone()),
                value: candidate.clone(),
            }))
            .to_generic();

            let status_event = {
                let status_event_id = UnitId::unit(&vault_status_desc).next().next();

                VaultMembershipObject::Membership(KvLogEvent {
                    key: KvKey {
                        obj_id: status_event_id,
                        obj_desc: vault_status_desc,
                    },
                    value: UserMembership::Member(UserDataMember(candidate.clone())),
                })
                .to_generic()
            };

            vec![unit_event, genesis_event, status_event]
        };
        commit_log.extend(vault_status_events);

        commit_log
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    use crate::{
        node::{
            db::actions::sign_up::SignUpAction,
        },
        node::common::model::user::user_creds::fixture::UserCredentialsFixture
    };
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;

    #[tokio::test]
    async fn test() -> Result<()> {
        let user_creds_fixture = UserCredentialsFixture::from(&DeviceCredentialsFixture::generate());

        let sign_up_action = SignUpAction {};
        let events = sign_up_action
            .accept(user_creds_fixture.client.user(), user_creds_fixture.server.device());

        assert_eq!(events.len(), 8);

        Ok(())
    }
}
