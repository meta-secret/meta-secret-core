use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    ApplicationState, VaultFullInfo,
    user::{common::*, user_creds::UserCreds},
    vault::vault_data::VaultAggregate,
};
use meta_secret_core::node::db::{
    actions::secret_deletion::SecretDeletionAction,
    descriptors::vault_descriptor::DeviceLogDescriptor,
    events::{
        kv_log_event::KvLogEvent,
        vault::{device_log_event::DeviceLogObject, vault_log_event::*},
    },
    repo::generic_db::*,
};
use serde_json::Value;

async fn request(h: &Harness, actor: &UserCreds) -> Result<DeleteSecretRequest> {
    let pass = h
        .vault
        .secrets
        .iter()
        .next()
        .context("actual Secret")?
        .clone();
    SecretDeletionAction {
        p_obj: h.p.clone(),
        user_creds: actor.clone(),
    }
    .request(pass)
    .await?;
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(actor.user_id()))
            .await?;
    events
        .into_iter()
        .find_map(|e| match e.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)) => {
                Some(request)
            }
            _ => None,
        })
        .context("actual persisted deletion request")
}
async fn settle(h: &mut Harness, count: usize, accepted: bool) -> Result<DeleteSecretDecision> {
    let request = request(h, &h.approver).await?;
    let actor = if count == 3 {
        h.initiator.clone()
    } else {
        h.approver.clone()
    };
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0
        .value
        .requests
        .insert(VaultActionRequestEvent::DeleteSecret(request.clone()));
    h.p.repo.replace(log.clone()).await?;
    SecretDeletionAction {
        p_obj: h.p.clone(),
        user_creds: actor.clone(),
    }
    .decide(&request.request_id, accepted)
    .await?;
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(actor.user_id()))
            .await?;
    let actual = events
        .into_iter()
        .find_map(|e| match e.0.value {
            VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(decision)) => {
                Some(decision)
            }
            _ => None,
        })
        .context("actual persisted eligible decision")?;
    log.0
        .value
        .updates
        .insert(VaultActionUpdateEvent::DeleteSecret(actual.clone()));
    let aggregate = VaultAggregate::build_from(log.0.value, h.vault.clone());
    assert_eq!(
        aggregate.vault.deletion_results.get(&request.request_id),
        Some(&actual)
    );
    h.vault = aggregate.vault;
    h.save_vault().await?;
    h.p.repo
        .replace(VaultLogObject(KvLogEvent {
            key: log.0.key,
            value: aggregate.events,
        }))
        .await?;
    Ok(actual)
}
async fn operation(h: &Harness, id: &str) -> Result<Value> {
    let ApplicationState::Vault(VaultFullInfo::Member(member)) =
        h.service().get_app_state().await?
    else {
        anyhow::bail!("actual current member missing")
    };
    serde_json::to_value(member)?["liveboardState"]["operations"]
        .as_array()
        .context("operations")?
        .iter()
        .find(|op| op["requestId"] == id)
        .cloned()
        .context("exact deletion operation")
}

#[tokio::test]
async fn authoritative_actual_local_and_peer_deletion_responders_survive_later_topology_and_tombstones()
-> Result<()> {
    for count in [1, 2, 3] {
        let mut h = Harness::new(count).await?;
        h.seed_secret("actual-response-authority").await?;
        let actual = settle(&mut h, count, count != 2).await?;
        if count == 3 {
            h.vault.users.remove(h.initiator.device_id());
            h.save_vault().await?;
        }
        let op = operation(&h, &actual.request.request_id).await?;
        assert_eq!(
            op["responderId"],
            actual.sender.user_data.device.device_id.to_string()
        );
        assert_eq!(op["resolutionSource"], "canonical");
        assert_eq!(
            op["outcome"],
            if actual.accepted {
                "accepted"
            } else {
                "declined"
            }
        );
        assert_eq!(op["canApprove"], false);
        assert_eq!(op["canDecline"], false);
    }
    Ok(())
}

#[tokio::test]
async fn authoritative_exact_result_wins_conflicting_log_only_result_for_same_request() -> Result<()>
{
    let mut h = Harness::new(3).await?;
    h.seed_secret("canonical-over-log").await?;
    let actual = settle(&mut h, 3, true).await?;
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0.value.deletion_results.insert(DeleteSecretDecision {
        sender: actual.request.sender.clone(),
        accepted: false,
        ..actual.clone()
    });
    h.p.repo.replace(log).await?;
    let op = operation(&h, &actual.request.request_id).await?;
    assert_eq!(op["outcome"], "accepted");
    assert_eq!(
        op["responderId"],
        actual.sender.user_data.device.device_id.to_string()
    );
    assert_eq!(op["resolutionSource"], "canonical");
    Ok(())
}

async fn log_only_retirement(count: usize) -> Result<()> {
    let mut h = Harness::new(count).await?;
    h.seed_secret("local-retirement-unknown").await?;
    let request = request(&h, &h.approver).await?;
    if count == 1 {
        h.vault = h
            .vault
            .clone()
            .update_membership(UserMembership::Member(UserDataMember::from(
                h.initiator.user(),
            )));
    } else {
        h.vault.users.remove(h.initiator.device_id());
    }
    h.save_vault().await?;
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0
        .value
        .requests
        .insert(VaultActionRequestEvent::DeleteSecret(request.clone()));
    log.0.value = log.0.value.retire_invalid_deletions(&h.vault);
    assert!(!h.vault.deletion_results.contains_key(&request.request_id));
    assert!(log.0.value.requests.is_empty());
    h.p.repo.replace(log).await?;
    let op = operation(&h, &request.request_id).await?;
    assert!(op.get("responderId").is_none_or(Value::is_null));
    assert_eq!(op["resolutionSource"], "unknown");
    assert_eq!(op["outcome"], "declined");
    assert_eq!(op["readiness"], "settled");
    Ok(())
}

#[tokio::test]
async fn log_only_one_device_retirement_is_not_upgraded_to_local_signed_response() -> Result<()> {
    log_only_retirement(1).await
}
#[tokio::test]
async fn log_only_two_device_retirement_is_not_upgraded_to_local_signed_response() -> Result<()> {
    log_only_retirement(2).await
}
