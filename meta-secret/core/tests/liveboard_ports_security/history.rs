use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    ApplicationState, VaultFullInfo, user::common::UserDataMember,
};
use meta_secret_core::node::db::{
    actions::secret_deletion::SecretDeletionAction,
    descriptors::vault_descriptor::DeviceLogDescriptor,
    events::vault::{device_log_event::DeviceLogObject, vault_log_event::*},
    repo::generic_db::*,
};
use serde_json::Value;

#[tokio::test]
async fn membership_retired_secret_deletion_never_fabricates_requester_as_declining_responder()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("retired-delete-authority").await?;
    SecretDeletionAction {
        p_obj: h.p.clone(),
        user_creds: h.initiator.clone(),
    }
    .request(pass)
    .await?;
    let actual: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.initiator.user_id()))
            .await?;
    let request = actual
        .into_iter()
        .find_map(|e| match e.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)) => {
                Some(request)
            }
            _ => None,
        })
        .context("actual persisted Delete Secret request")?;
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0
        .value
        .requests
        .insert(VaultActionRequestEvent::DeleteSecret(request.clone()));
    // Real existing Core retirement mechanism: the topology changed; no device
    // signed a Delete decision, and no real peer may be invented as responder.
    h.vault.users.remove(h.target.device_id());
    h.save_vault().await?;
    log.0.value = log.0.value.retire_invalid_deletions(&h.vault);
    assert!(log.0.value.requests.is_empty());
    let retired = log
        .0
        .value
        .deletion_results
        .iter()
        .next()
        .context("real retirement result")?;
    assert!(!retired.accepted);
    assert_eq!(retired.sender, request.sender);
    assert!(
        h.vault
            .ensure_can_delete_secret(&request, Some(&retired.sender))
            .is_err()
    );
    h.p.repo.replace(log).await?;
    let ApplicationState::Vault(VaultFullInfo::Member(member)) =
        h.service().get_app_state().await?
    else {
        anyhow::bail!("real current member missing")
    };
    let projection = serde_json::to_value(member)?["liveboardState"].clone();
    let op = projection["operations"]
        .as_array()
        .context("operations")?
        .iter()
        .find(|op| op["requestId"] == request.request_id)
        .context("exact retired operation")?;
    assert_eq!(op["canApprove"], false);
    assert_eq!(op["canDecline"], false);
    assert!(
        op.get("responderId").is_none_or(Value::is_null),
        "membership retirement must not claim the requester sent a Decline"
    );
    assert_eq!(op["resolutionSource"], "unknown");
    Ok(())
}
