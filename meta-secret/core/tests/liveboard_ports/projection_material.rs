use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    ApplicationState, VaultFullInfo,
    crypto::aead::EncryptedMessage,
    secret::{SecretDistributionData, SsClaim, SsDistributionStatus},
    user::common::UserDataMember,
};
use meta_secret_core::node::db::{
    descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
    events::{
        kv_log_event::*,
        shared_secret_event::SsWorkflowObject,
        vault::liveboard_metadata::{DecisionOutcome, RecoveryDecisionRecord},
    },
    objects::persistent_shared_secret::PersistentSharedSecret,
    repo::generic_db::*,
};
use meta_secret_core::secret::shared_secret::PlainText;
use serde_json::Value;

async fn state(h: &Harness) -> Result<Value> {
    let ApplicationState::Vault(VaultFullInfo::Member(member)) =
        h.service().get_app_state().await?
    else {
        anyhow::bail!("missing actual member state")
    };
    serde_json::to_value(member)?["liveboardState"]
        .as_object()
        .cloned()
        .map(Value::Object)
        .context("RED: real get_app_state must return allowlisted projection")
}
fn secret<'a>(state: &'a Value, pass: &str) -> Result<&'a Value> {
    state["secrets"]
        .as_array()
        .context("secret list")?
        .iter()
        .find(|item| item["secretId"] == pass)
        .context("exact secret")
}
async fn pending_claim(
    h: &Harness,
    pass: meta_secret_core::node::common::model::meta_pass::MetaPasswordId,
) -> Result<SsClaim> {
    let vm = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.approver.user()))?;
    let claim = vm.create_recovery_claim(pass);
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    Ok(claim)
}

#[tokio::test]
async fn projection_wrong_threshold_after_membership_change_cannot_become_ready_by_count_alone()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("wrong-threshold").await?;
    assert_eq!(
        h.share(&pass, &h.approver).await?.share_blocks[0]
            .config
            .threshold,
        2
    );
    h.vault.users.remove(h.target.device_id());
    h.save_vault().await?;
    let current = state(&h).await?;
    assert_ne!(
        secret(&current, &pass.id_str())?["primaryAction"],
        "showLocal"
    );
    assert_ne!(
        secret(&current, &pass.id_str())?["readiness"],
        "ready",
        "canonical two-member count is not completed local resharing"
    );
    Ok(())
}

#[tokio::test]
async fn projection_real_recovery_packet_reports_exact_accepted_claim_and_verified_responder()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("usable-recovered").await?;
    let mut claim = pending_claim(&h, pass.clone()).await?;
    claim
        .status
        .statuses
        .insert(h.initiator.device_id().clone(), SsDistributionStatus::Sent);
    claim
        .status
        .statuses
        .insert(h.target.device_id().clone(), SsDistributionStatus::Declined);
    let share = h.share(&pass, &h.initiator).await?;
    let encrypted = h
        .initiator
        .device_creds
        .key_manager()?
        .transport
        .encrypt_string(
            PlainText::from(share.as_json()?),
            &h.approver.device().keys.transport_pk,
        )?;
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == *h.initiator.device_id())
        .context("actual targeted receiver")?;
    h.p.repo
        .save(SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(id)),
            value: SecretDistributionData {
                vault_name: h.vault.vault_name.clone(),
                claim_id: claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share: encrypted },
            },
        }))
        .await?;
    let ss = PersistentSharedSecret::from(h.p.clone());
    ss.save_ss_log_event(claim.clone()).await?;
    let mut log = ss.get_ss_log_obj(h.vault.vault_name.clone()).await?;
    log.liveboard_recovery_results.insert(
        claim.id.clone(),
        RecoveryDecisionRecord {
            claim_id: claim.id.clone(),
            responder_id: h.initiator.device_id().clone(),
            outcome: DecisionOutcome::Accepted,
            source: None,
        },
    );
    let obj = ss
        .create_new_ss_log_object(log, h.vault.vault_name.clone())
        .await?;
    h.p.repo.save(obj).await?;
    let current = state(&h).await?;
    let s = secret(&current, &pass.id_str())?;
    assert_eq!(s["primaryAction"], "showRecovered");
    assert_eq!(s["recoveryClaimId"], claim.id.wasm_id_str());
    assert_eq!(s["readiness"], "ready");
    let op = current["operations"]
        .as_array()
        .context("ops")?
        .iter()
        .find(|op| op["requestId"] == claim.id.wasm_id_str())
        .context("exact recovery operation")?;
    assert_eq!(op["responderId"], h.initiator.device_id().to_string());
    assert_eq!(op["requesterId"], h.approver.device_id().to_string());
    assert!(!serde_json::to_string(&current)?.contains(VALUE));
    Ok(())
}

#[tokio::test]
async fn projection_duplicate_active_claims_fail_unavailable_instead_of_selecting_random_hashmap_entry()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("ambiguous-claims").await?;
    let first = pending_claim(&h, pass.clone()).await?;
    let second = pending_claim(&h, pass.clone()).await?;
    assert_ne!(first.id, second.id);
    let current = state(&h).await?;
    let s = secret(&current, &pass.id_str())?;
    assert_eq!(s["primaryAction"], "unavailable");
    assert!(s.get("recoveryClaimId").is_none_or(Value::is_null));
    assert_ne!(s["readiness"], "ready");
    Ok(())
}

#[tokio::test]
async fn projection_historical_decline_without_verified_sidecar_never_guesses_any_peer_responder()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("historical-decline").await?;
    let mut claim = pending_claim(&h, pass).await?;
    for status in claim.status.statuses.values_mut() {
        *status = SsDistributionStatus::Declined;
    }
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    let current = state(&h).await?;
    if let Some(op) = current["operations"]
        .as_array()
        .context("operations")?
        .iter()
        .find(|op| op["requestId"] == claim.id.wasm_id_str())
    {
        assert!(op.get("responderId").is_none_or(Value::is_null));
        assert_eq!(op["resolutionSource"], "unknown");
    }
    Ok(())
}
