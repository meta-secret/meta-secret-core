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

async fn state(h: &Harness) -> Result<Value> {
    let ApplicationState::Vault(VaultFullInfo::Member(member)) =
        h.service().get_app_state().await?
    else {
        anyhow::bail!("actual member state missing")
    };
    let value = serde_json::to_value(member)?;
    value["liveboardState"]
        .as_object()
        .cloned()
        .map(Value::Object)
        .context("RED: exact operation projection missing")
}
fn operation<'a>(state: &'a Value, id: &str) -> Result<&'a Value> {
    state["operations"]
        .as_array()
        .context("operations array")?
        .iter()
        .find(|op| op["requestId"] == id)
        .context("current canonical operation missing")
}

#[tokio::test]
async fn projection_removal_uses_canonical_binding_and_only_the_remaining_member_can_answer()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    h.seed_secret("remove-projection").await?;
    h.seed_removal().await?;
    let current = state(&h).await?;
    let op = operation(&current, REQUEST)?;
    assert_eq!(op["kind"], "removeDevice");
    assert_eq!(op["requesterId"], h.initiator.device_id().to_string());
    assert_eq!(op["targetDeviceId"], h.target.device_id().to_string());
    assert_eq!(op["canApprove"], true);
    assert_eq!(op["canDecline"], true);
    assert_eq!(
        op["eligiblePeerIds"],
        serde_json::json!([h.approver.device_id().to_string()])
    );
    assert!(op.get("responderId").is_none_or(Value::is_null));
    Ok(())
}

#[tokio::test]
async fn projection_join_exposes_only_the_current_uuid_and_real_candidate() -> Result<()> {
    let mut h = Harness::new(1).await?;
    let request = super::exact_join::pending(&mut h).await?;
    let current = state(&h).await?;
    let op = operation(&current, &request.request_id)?;
    assert_eq!(op["kind"], "join");
    assert_eq!(
        op["requesterId"],
        request.candidate.device.device_id.to_string()
    );
    assert_eq!(op["canApprove"], true);
    assert_eq!(op["canDecline"], true);
    let pending: Vec<_> = current["operations"]
        .as_array()
        .context("ops")?
        .iter()
        .filter(|op| op["kind"] == "join" && op["canApprove"] == true)
        .collect();
    assert_eq!(
        pending.len(),
        1,
        "retained historical result must not be actionable pending Join"
    );
    Ok(())
}

#[tokio::test]
async fn projection_delete_uses_actual_request_uuid_secret_and_signed_sender_with_core_actor_policy()
-> Result<()> {
    for own_request in [false, true] {
        let mut h = Harness::new(3).await?;
        let pass = h.seed_secret("delete-projection").await?;
        let actor = if own_request {
            h.approver.clone()
        } else {
            h.initiator.clone()
        };
        SecretDeletionAction {
            p_obj: h.p.clone(),
            user_creds: actor.clone(),
        }
        .request(pass.clone())
        .await?;
        let events: Vec<DeviceLogObject> =
            h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(actor.user_id()))
                .await?;
        let request = events
            .into_iter()
            .find_map(|event| match event.0.value {
                VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)) => {
                    Some(request)
                }
                _ => None,
            })
            .context("actual deletion request")?;
        let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
        log.0
            .value
            .requests
            .insert(VaultActionRequestEvent::DeleteSecret(request.clone()));
        h.p.repo.replace(log).await?;
        let current = state(&h).await?;
        let op = operation(&current, &request.request_id)?;
        assert_eq!(op["kind"], "deleteSecret");
        assert_eq!(op["secretId"], pass.id_str());
        assert_eq!(op["requesterId"], actor.device_id().to_string());
        assert_eq!(op["canApprove"], !own_request);
        assert_eq!(op["canDecline"], !own_request);
        assert!(op.get("responderId").is_none_or(Value::is_null));
        let s = current["secrets"]
            .as_array()
            .context("secrets")?
            .iter()
            .find(|s| s["secretId"] == pass.id_str())
            .context("pass")?;
        assert_eq!(s["deletionRequestId"], request.request_id);
        assert_eq!(s["canDelete"], false);
    }
    Ok(())
}
