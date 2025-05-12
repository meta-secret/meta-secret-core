use crate::node::common::model::user::common::{
    UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::events::vault::vault_log_event::{JoinClusterEvent, UpdateMembershipEvent};
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::bail;
use anyhow::Result;
use std::sync::Arc;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub enum JoinActionUpdate {
    Accept,
    Decline,
}

pub struct JoinAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub member: VaultMember,
}

impl<Repo: KvLogEventRepo> JoinAction<Repo> {
    pub async fn update(
        &self,
        join_request: JoinClusterEvent,
        upd: JoinActionUpdate,
    ) -> Result<()> {
        let candidate_membership = self.member.vault.membership(join_request.candidate.clone());
        let p_device_log = PersistentDeviceLog::from(self.p_obj.clone());

        match candidate_membership {
            UserMembership::Outsider(outsider) => match outsider.status {
                UserDataOutsiderStatus::NonMember | UserDataOutsiderStatus::Pending => {
                    let update = match upd {
                        JoinActionUpdate::Accept => UserMembership::Member(UserDataMember {
                            user_data: outsider.user_data,
                        }),
                        JoinActionUpdate::Decline => UserMembership::Outsider(UserDataOutsider {
                            user_data: outsider.user_data,
                            status: UserDataOutsiderStatus::Declined,
                        }),
                    };

                    let update_event = UpdateMembershipEvent {
                        request: join_request,
                        sender: self.member.member.clone(),
                        update,
                    };

                    p_device_log
                        .save_updated_membership_event(update_event)
                        .await
                }
                UserDataOutsiderStatus::Declined => {
                    bail!("User request already declined")
                }
            },
            UserMembership::Member(_) => {
                bail!("Membership cannot be accepted. Invalid state")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{UserDataOutsider, UserDataOutsiderStatus};
    use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;

    /// Tests successful acceptance of a join request for a non-member user
    #[tokio::test]
    async fn test_accept_join_request_success() -> Result<()> {
        // Setup fixture
        let registry = FixtureRegistry::empty();
        let p_obj = registry.state.p_obj.client.clone();
        let vault_data_fixture = &registry.state.vault_data;

        // Create a new candidate user who is not yet a member
        let candidate_creds = registry.state.user_creds.vd.clone();
        let candidate_user = candidate_creds.user();

        // Create custom vault data where the candidate is a non-member
        let mut custom_vault = vault_data_fixture.full_membership.clone();
        let outsider = UserDataOutsider::non_member(candidate_user.clone());
        custom_vault = custom_vault.update_membership(UserMembership::Outsider(outsider.clone()));

        // Create a vault member with the custom vault data
        let member = VaultMember {
            member: vault_data_fixture.client_membership.user_data_member(),
            vault: custom_vault,
        };

        // Create the action
        let action = JoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.update(join_request, JoinActionUpdate::Accept).await;

        // Verify result is successful
        assert!(result.is_ok(), "Accept join request should succeed");

        // Check if device log has an entry (optional but more thorough validation)
        let obj_desc = DeviceLogDescriptor::from(member.member.user().user_id());
        let tail_id = p_obj.find_tail_id_by_obj_desc(obj_desc).await?;
        assert!(
            tail_id.is_some(),
            "Expected device log to have an entry after accepting join request"
        );

        Ok(())
    }

    /// Tests that a user who has been declined cannot be accepted
    #[tokio::test]
    async fn test_accept_join_request_already_declined() -> Result<()> {
        // Setup fixture
        let registry = FixtureRegistry::empty();
        let p_obj = registry.state.p_obj.client.clone();
        let vault_data_fixture = &registry.state.vault_data;

        // Create a new candidate user who is in DECLINED state
        let candidate_creds = registry.state.user_creds.vd.clone();
        let candidate_user = candidate_creds.user();

        // Create a declined outsider
        let mut outsider = UserDataOutsider::non_member(candidate_user.clone());
        outsider.status = UserDataOutsiderStatus::Declined;

        // Create custom vault data where the candidate is declined
        let mut custom_vault = vault_data_fixture.full_membership.clone();
        custom_vault = custom_vault.update_membership(UserMembership::Outsider(outsider));

        // Create a vault member with the custom vault data
        let member = VaultMember {
            member: vault_data_fixture.client_membership.user_data_member(),
            vault: custom_vault,
        };

        // Create the action
        let action = JoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.update(join_request, JoinActionUpdate::Accept).await;

        // Verify result
        assert!(
            result.is_err(),
            "Accept join request should fail for declined user"
        );
        let error = result.unwrap_err().to_string();
        assert_eq!(
            error, "User request already declined",
            "Error message should indicate user request was declined"
        );

        Ok(())
    }

    /// Tests that a user who is already a member cannot be accepted
    #[tokio::test]
    async fn test_accept_join_request_already_member() -> Result<()> {
        // Setup fixture
        let registry = FixtureRegistry::empty();
        let p_obj = registry.state.p_obj.client.clone();
        let vault_data_fixture = &registry.state.vault_data;

        // Use an existing member as the candidate
        let candidate_user = vault_data_fixture.client_b_membership.user_data().clone();

        // Use the standard vault data where the candidate is already a member
        let member = vault_data_fixture.client_vault_member.clone();

        // Create the action
        let action = JoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.update(join_request, JoinActionUpdate::Accept).await;

        // Verify result
        assert!(
            result.is_err(),
            "Accept join request should fail for existing member"
        );
        let error = result.unwrap_err().to_string();
        assert_eq!(
            error, "Membership cannot be accepted. Invalid state",
            "Error message should indicate membership cannot be accepted"
        );

        Ok(())
    }
}
