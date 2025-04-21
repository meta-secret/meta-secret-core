use crate::node::common::model::user::common::UserDataMember;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use log::info;
use tracing_attributes::instrument;

pub struct SignUpAction;

impl SignUpAction {
    #[instrument(skip(self))]
    pub fn accept(&self, candidate: UserDataMember) -> Vec<GenericKvLogEvent> {
        info!("Create new vault");

        let vault_name = candidate.user_data.vault_name();

        let vault_log_event = VaultLogObject::create(candidate.clone()).to_generic();

        let vault_event = {
            let vault_event = VaultObject::sign_up(vault_name.clone(), candidate);
            vault_event.to_generic()
        };

        vec![vault_log_event, vault_event]
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    use crate::crypto::keys::fixture::KeyManagerFixture;
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::{
        node::common::model::user::user_creds::fixture::UserCredentialsFixture,
        node::db::actions::sign_up::action::SignUpAction,
    };

    #[tokio::test]
    async fn test_sing_up() -> Result<()> {
        let km = KeyManagerFixture::generate();
        let device_creds = &DeviceCredentialsFixture::from_km(km);
        let user_creds_fixture = UserCredentialsFixture::from(device_creds);

        let sign_up_action = SignUpAction;
        let user_data_member = UserDataMember::from(user_creds_fixture.client.user());
        let events = sign_up_action.accept(user_data_member.clone());

        assert_eq!(events.len(), 2);

        // Verify that we have both a VaultLog and a Vault event in the correct order
        let mut vault_log_verified = false;
        let mut vault_event_verified = false;

        let expected_vault_name = user_data_member.user_data.vault_name();

        for (index, event) in events.iter().enumerate() {
            match event {
                GenericKvLogEvent::VaultLog(obj) => {
                    // Verify this is the first event (index 0)
                    assert_eq!(index, 0, "VaultLog should be the first event");

                    // Verify the vault name matches
                    assert!(
                        obj.0
                            .key
                            .obj_id
                            .fqdn
                            .obj_instance
                            .contains(&expected_vault_name.to_string()),
                        "Vault name in VaultLog event should match user's vault name"
                    );
                    vault_log_verified = true;
                }
                GenericKvLogEvent::Vault(obj) => {
                    // Verify this is the second event (index 1)
                    assert_eq!(index, 1, "Vault should be the second event");

                    // Verify vault name matches
                    let is_expected_vault_name = obj
                        .0
                        .key
                        .obj_id
                        .fqdn
                        .obj_instance
                        .contains(&expected_vault_name.to_string());
                    assert!(
                        is_expected_vault_name,
                        "Vault name in Vault event should match user's vault name"
                    );

                    // Verify vault data contains the correct user information
                    let vault_data = &obj.0.value;

                    // Check vault name matches
                    assert_eq!(
                        vault_data.vault_name.to_string(),
                        expected_vault_name.to_string(),
                        "Vault name in vault data should match"
                    );

                    // Check user device ID is in the users map
                    let device_id = user_data_member.user_data.device.device_id.clone();
                    assert!(
                        vault_data.users.contains_key(&device_id),
                        "User's device ID should be in the vault users map"
                    );

                    // Verify user is a member
                    if let Some(membership) = vault_data.users.get(&device_id) {
                        match membership {
                            UserMembership::Member(member) => {
                                assert_eq!(
                                    member.user_data.vault_name.to_string(),
                                    expected_vault_name.to_string(),
                                    "User membership should have the correct vault name"
                                );
                            }
                            _ => panic!("Expected user to be a Member, not an Outsider"),
                        }
                    }

                    vault_event_verified = true;
                }
                _ => panic!("Unexpected event type: {:?}", event),
            }
        }

        assert!(vault_log_verified, "VaultLog event was not verified");
        assert!(vault_event_verified, "Vault event was not verified");

        Ok(())
    }
}
