use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::events::vault::vault_log_event::{VaultActionEvents, VaultActionRequestEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretDeletionClientStatus {
    Pending,
    NeedApprove,
    Done,
    Declined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretDeletionDecisionInput {
    pub request_id: String,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretDeletionState {
    pub pass_id: MetaPasswordId,
    pub secret_id: String,
    pub request_id: Option<String>,
    pub requires_approval: bool,
    pub client_status: Option<SecretDeletionClientStatus>,
}

pub fn deletion_states(
    vault: &VaultData,
    events: &VaultActionEvents,
    device: &DeviceId,
) -> Vec<SecretDeletionState> {
    let mut states: Vec<_> = vault
        .secrets
        .iter()
        .map(|pass| SecretDeletionState {
            pass_id: pass.clone(),
            secret_id: pass.id_str(),
            request_id: None,
            requires_approval: vault.members().len() >= 3,
            client_status: None,
        })
        .collect();
    for event in &events.requests {
        if let VaultActionRequestEvent::DeleteSecret(request) = event {
            if vault.deletion_results.contains_key(&request.request_id) {
                continue;
            }
            if vault.ensure_can_delete_secret(request, None).is_err() {
                continue;
            }
            let status = if request.sender.user_data.device.device_id == *device {
                SecretDeletionClientStatus::Pending
            } else if request.members.contains(device) {
                SecretDeletionClientStatus::NeedApprove
            } else {
                continue;
            };
            states.push(SecretDeletionState {
                pass_id: request.pass_id.clone(),
                secret_id: request.pass_id.id_str(),
                request_id: Some(request.request_id.clone()),
                requires_approval: request.requires_approval(),
                client_status: Some(status),
            });
        }
    }
    let mut results: std::collections::HashMap<_, _> = events
        .deletion_results
        .iter()
        .map(|result| (result.request.request_id.clone(), result))
        .collect();
    results.extend(
        vault
            .deletion_results
            .iter()
            .map(|(id, result)| (id.clone(), result)),
    );
    for result in results.values() {
        if !result.request.members.contains(device) {
            continue;
        }
        states.push(SecretDeletionState {
            pass_id: result.request.pass_id.clone(),
            secret_id: result.request.pass_id.id_str(),
            request_id: Some(result.request.request_id.clone()),
            requires_approval: result.request.requires_approval(),
            client_status: Some(if result.accepted {
                SecretDeletionClientStatus::Done
            } else {
                SecretDeletionClientStatus::Declined
            }),
        });
    }
    states.sort_by(|a, b| (&a.pass_id.name, &a.request_id).cmp(&(&b.pass_id.name, &b.request_id)));
    states
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::db::events::vault::vault_log_event::{
        DeleteSecretDecision, DeleteSecretRequest, VaultActionUpdateEvent,
    };

    #[test]
    fn canonical_deletion_outcome_survives_missing_or_stale_transient_log() {
        use crate::node::common::model::vault::vault_data::VaultAggregate;
        let fixture = FixtureRegistry::empty();
        let a = UserDataMember::from(fixture.state.user_creds.client.user());
        let b = UserDataMember::from(fixture.state.user_creds.client_b.user());
        let c = UserDataMember::from(fixture.state.user_creds.vd.user());
        let pass = MetaPasswordId::build_from_str("atomic-delete-outcome");
        let vault = VaultData::from(a.clone())
            .add_secret(pass.clone())
            .update_membership(UserMembership::Member(b.clone()))
            .update_membership(UserMembership::Member(c.clone()));
        let request = DeleteSecretRequest::new(a.clone(), pass.clone(), &vault);
        let pending = VaultActionEvents::default()
            .request(VaultActionRequestEvent::DeleteSecret(request.clone()));
        let accepted =
            pending
                .clone()
                .apply(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                    request: request.clone(),
                    sender: b,
                    accepted: true,
                }));
        let canonical = VaultAggregate::build_from(accepted, vault).vault;
        assert!(canonical.deleted_secrets.contains(&pass));
        assert!(
            canonical
                .deletion_results
                .get(&request.request_id)
                .unwrap()
                .accepted
        );
        let stale = pending.apply(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
            request: request.clone(),
            sender: c,
            accepted: false,
        }));
        for events in [VaultActionEvents::default(), stale] {
            let states = deletion_states(&canonical, &events, &a.user_data.device.device_id);
            let state = states
                .iter()
                .find(|state| state.request_id.as_deref() == Some(&request.request_id))
                .unwrap();
            assert_eq!(state.client_status, Some(SecretDeletionClientStatus::Done));
            assert_eq!(
                states
                    .iter()
                    .filter(|state| state.request_id.as_deref() == Some(&request.request_id))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn deletion_client_instructions_follow_core_decision_and_membership() {
        let fixture = FixtureRegistry::empty();
        let a = UserDataMember::from(fixture.state.user_creds.client.user());
        let b = UserDataMember::from(fixture.state.user_creds.client_b.user());
        let c = UserDataMember::from(fixture.state.user_creds.vd.user());
        let pass = MetaPasswordId::build_from_str("delete-status");
        let vault = VaultData::from(a.clone())
            .add_secret(pass.clone())
            .update_membership(UserMembership::Member(b.clone()))
            .update_membership(UserMembership::Member(c.clone()));
        let request = DeleteSecretRequest::new(a.clone(), pass, &vault);
        let events = VaultActionEvents::default()
            .request(VaultActionRequestEvent::DeleteSecret(request.clone()));
        let status = |vault: &VaultData, events: &VaultActionEvents, member: &UserDataMember| {
            deletion_states(vault, events, &member.user_data.device.device_id)
                .into_iter()
                .find(|state| state.request_id.as_deref() == Some(&request.request_id))
                .unwrap()
                .client_status
        };
        assert_eq!(
            status(&vault, &events, &a),
            Some(SecretDeletionClientStatus::Pending)
        );
        assert_eq!(
            status(&vault, &events, &b),
            Some(SecretDeletionClientStatus::NeedApprove)
        );
        let declined = events
            .clone()
            .apply(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                request: request.clone(),
                sender: b.clone(),
                accepted: false,
            }))
            .complete();
        assert_eq!(
            status(&vault, &declined, &a),
            Some(SecretDeletionClientStatus::Declined)
        );
        let accepted = events
            .clone()
            .apply(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                request: request.clone(),
                sender: b,
                accepted: true,
            }))
            .complete();
        let deleted = vault.clone().delete_secret(request.pass_id.clone());
        assert_eq!(
            status(&deleted, &accepted, &c),
            Some(SecretDeletionClientStatus::Done)
        );
        let mut changed = vault;
        changed.users.remove(&c.user_data.device.device_id);
        let invalidated = events.retire_invalid_deletions(&changed);
        assert!(invalidated.requests.is_empty());
        assert_eq!(
            status(&changed, &invalidated, &a),
            Some(SecretDeletionClientStatus::Declined)
        );
    }
}
