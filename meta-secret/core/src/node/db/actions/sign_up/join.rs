use crate::node::common::model::user::common::{UserDataOutsiderStatus, UserMembership};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::events::vault::vault_log_event::JoinClusterEvent;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::bail;
use anyhow::Result;
use std::sync::Arc;

pub struct AcceptJoinAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub member: VaultMember,
}

impl<Repo: KvLogEventRepo> AcceptJoinAction<Repo> {
    pub async fn accept(&self, join_request: JoinClusterEvent) -> Result<()> {
        let candidate_membership = self.member.vault.membership(join_request.candidate.clone());

        match candidate_membership {
            UserMembership::Outsider(outsider) => match outsider.status {
                UserDataOutsiderStatus::NonMember => {
                    let p_device_log = PersistentDeviceLog {
                        p_obj: self.p_obj.clone(),
                    };

                    p_device_log
                        .save_accept_join_request_event(
                            join_request,
                            self.member.member.clone(),
                            outsider,
                        )
                        .await
                }
                UserDataOutsiderStatus::Pending => {
                    bail!("User already in pending state")
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
        let action = AcceptJoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.accept(join_request).await;

        // Verify result
        assert!(result.is_ok(), "Accept join request should succeed");

        Ok(())
    }

    #[tokio::test]
    async fn test_accept_join_request_already_pending() -> Result<()> {
        // Setup fixture
        let registry = FixtureRegistry::empty();
        let p_obj = registry.state.p_obj.client.clone();
        let vault_data_fixture = &registry.state.vault_data;

        // Create a new candidate user who is in PENDING state
        let candidate_creds = registry.state.user_creds.vd.clone();
        let candidate_user = candidate_creds.user();
        
        // Create a pending outsider
        let mut outsider = UserDataOutsider::non_member(candidate_user.clone());
        outsider.status = UserDataOutsiderStatus::Pending;

        // Create custom vault data where the candidate is pending
        let mut custom_vault = vault_data_fixture.full_membership.clone();
        custom_vault = custom_vault.update_membership(UserMembership::Outsider(outsider));

        // Create a vault member with the custom vault data
        let member = VaultMember {
            member: vault_data_fixture.client_membership.user_data_member(),
            vault: custom_vault,
        };

        // Create the action
        let action = AcceptJoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.accept(join_request).await;

        // Verify result
        assert!(result.is_err(), "Accept join request should fail for pending user");
        assert_eq!(
            result.unwrap_err().to_string(),
            "User already in pending state",
            "Error message should indicate user is pending"
        );

        Ok(())
    }

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
        let action = AcceptJoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.accept(join_request).await;

        // Verify result
        assert!(result.is_err(), "Accept join request should fail for declined user");
        assert_eq!(
            result.unwrap_err().to_string(),
            "User request already declined",
            "Error message should indicate user request was declined"
        );

        Ok(())
    }

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
        let action = AcceptJoinAction {
            p_obj: p_obj.clone(),
            member: member.clone(),
        };

        // Create the join request
        let join_request = JoinClusterEvent {
            candidate: candidate_user.clone(),
        };

        // Execute the function
        let result = action.accept(join_request).await;

        // Verify result
        assert!(result.is_err(), "Accept join request should fail for existing member");
        assert_eq!(
            result.unwrap_err().to_string(),
            "Membership cannot be accepted. Invalid state",
            "Error message should indicate membership cannot be accepted"
        );

        Ok(())
    }
}
