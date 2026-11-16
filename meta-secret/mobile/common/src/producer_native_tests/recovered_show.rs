//! Actual native JSON/SQLite/loopback receipt contract, not native UI E2E.
use super::{
    OWNER,
    harness::{CanonicalDelta, Harness, OTHER_VALUE},
    seed_fixture,
};
use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    common::model::{
        crypto::aead::EncryptedMessage,
        meta_pass::MetaPasswordId,
        secret::{SecretDistributionData, SsClaim, SsDistributionStatus},
        user::common::UserDataMember,
    },
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::{
            kv_log_event::*,
            shared_secret_event::SsWorkflowObject,
            vault::liveboard_metadata::{DecisionOutcome, RecoveryDecisionRecord},
        },
        objects::{
            persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
        },
        repo::generic_db::*,
    },
};
use meta_secret_core::secret::shared_secret::PlainText;
use serde_json::Value;
use std::{io::Write, sync::Arc};

// Seed actual existing distributor output; no receipt or matching proof JSON.
pub(super) async fn seed_accepted(
    h: &seed_fixture::Harness,
    pass: &MetaPasswordId,
) -> Result<SsClaim> {
    let local = h.share(pass, &h.approver).await?;
    let remote = h.share(pass, &h.initiator).await?;
    ensure!(
        h.vault.members().len() == 3 && local.share_id != remote.share_id,
        "native fixture requires three real members and distinct own/remote shares"
    );
    for share in [&local, &remote] {
        ensure!(
            !share.share_blocks.is_empty()
                && share.share_blocks.iter().all(|block| {
                    block.config.threshold == 2 && block.config.number_of_shares == 3
                }),
            "native fixture requires actual n3/k2 material"
        );
    }
    let mut claim = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.approver.user()))?
        .create_recovery_claim(pass.clone());
    claim
        .status
        .statuses
        .insert(h.initiator.device_id().clone(), SsDistributionStatus::Sent);
    claim
        .status
        .statuses
        .insert(h.target.device_id().clone(), SsDistributionStatus::Declined);
    let encrypted = h
        .initiator
        .device_creds
        .key_manager()?
        .transport
        .encrypt_string(
            PlainText::from(remote.as_json()?),
            &h.approver.device().keys.transport_pk,
        )?;
    let recovery_id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == *h.initiator.device_id())
        .context("native fixture exact approved receiver")?;
    h.p.repo
        .save(SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(recovery_id)),
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
    h.p.repo
        .save(
            ss.create_new_ss_log_object(log, h.vault.vault_name.clone())
                .await?,
        )
        .await?;
    Ok(claim)
}

fn projection() -> Result<Value> {
    let state: Value = serde_json::from_str(&crate::json_api::get_state())?;
    ensure!(
        state["success"] == true,
        "actual native JSON state must succeed"
    );
    state["message"]["state"]["vault"]["member"]["liveboardState"]
        .as_object()
        .cloned()
        .map(Value::Object)
        .context("actual native Member projection")
}
fn operation<'a>(state: &'a Value, claim: &SsClaim) -> Result<&'a Value> {
    state["operations"]
        .as_array()
        .context("native operations")?
        .iter()
        .find(|op| op["requestId"] == claim.id.wasm_id_str())
        .context("exact native Recover operation")
}
fn secret<'a>(state: &'a Value, pass: &MetaPasswordId) -> Result<&'a Value> {
    state["secrets"]
        .as_array()
        .context("native secrets")?
        .iter()
        .find(|item| item["secretId"] == pass.id_str())
        .context("exact native Secret")
}

fn actual_json_recovery(h: &Harness) -> Result<()> {
    let claim = h
        .recovery
        .as_ref()
        .context("native fixture actual Recover claim")?;
    let before = projection()?;
    ensure!(
        secret(&before, &h.secret)?["primaryAction"] == "showRecovered"
            && secret(&before, &h.secret)?["readiness"] == "ready",
        "native JSON fixture must be currently actionable local+remote recovery"
    );
    let accepted = operation(&before, claim)?;
    ensure!(
        accepted["clientStatus"] == "accepted",
        "native actual Accepted operation"
    );
    let context = accepted
        .get("revealContext")
        .filter(|value| value.is_object())
        .context("current Core Accepted source context must exist before JSON Show")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local_ss = PersistentSharedSecret::from(h.p.clone());
    let sources = runtime.block_on(local_ss.get_recoveries(claim.clone()))?;
    ensure!(
        sources.len() == 1,
        "one actual addressed native Recovery ciphertext source"
    );

    // Exact existing public JSON route, with its normal FFI lock/runtime, manager,
    // Core guard/reconstruction, signed completion and synchronization.
    let response: Value =
        serde_json::from_str(&crate::json_api::show_recovered(claim.id.wasm_id_str()))?;
    ensure!(
        response["success"] == true,
        "legacy native JSON Show must succeed before receipt RED"
    );
    ensure!(
        response["message"]["secret"].as_str() == Some(seed_fixture::VALUE),
        "legacy native JSON must preserve exact fixture plaintext"
    );
    let server_ss =
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())));
    let server = runtime.block_on(server_ss.get_ss_log_obj(h.actor.vault_name.clone()))?;
    let terminal = server
        .claims
        .get(&claim.id)
        .context("actual native server completion")?;
    ensure!(
        terminal.status.statuses.get(h.actor.device_id()) == Some(&SsDistributionStatus::Delivered),
        "actual signed native completion must persist requester Delivered"
    );
    ensure!(
        terminal.approved_recovery_receiver()? == claim.approved_recovery_receiver()?,
        "actual completion must retain exact approved receiver"
    );
    let after = projection()?;
    let done = operation(&after, claim)?;
    ensure!(
        done["clientStatus"] == "done" && secret(&after, &h.secret)?["primaryAction"] == "recover",
        "normal native JSON sync must expose exact Done/recover transition"
    );
    ensure!(
        done.get("revealContext") == Some(context),
        "real Accepted/Done Core contexts must agree"
    );
    ensure!(
        runtime.block_on(local_ss.get_recoveries(claim.clone()))? == sources,
        "normal native completion/sync preserves actual consumed Recovery ciphertext"
    );

    // First new behavior assertion runs only after every successful real path above.
    let receipt = response["message"]
        .get("revealReceipt")
        .filter(|value| value.is_object())
        .context("RED: successful native JSON recovered Show must return captured revealReceipt")?;
    ensure!(
        receipt["version"] == 1
            && receipt["secretId"] == h.secret.id_str()
            && receipt["recoveryClaimId"] == claim.id.wasm_id_str()
            && receipt["requesterId"] == h.actor.device_id().to_string(),
        "exact native receipt call identity"
    );
    ensure!(
        receipt.get("revealContext") == Some(context),
        "receipt binds original accepted sources"
    );
    Ok(())
}

#[test]
fn actual_native_json_recovered_show_returns_captured_receipt() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ensure!(
        crate::mobile_manager::MobileApplicationManager::get_global_instance().is_none(),
        "native test requires exclusive unused singleton"
    );
    let h = Harness::new_recovery()?;
    let result = actual_json_recovery(&h);
    // Cleanup precedes even the expected RED result: restore singleton, stop and
    // join service, then stop/join ephemeral loopback and remove private TempDir.
    drop(h);
    ensure!(
        crate::mobile_manager::MobileApplicationManager::get_global_instance().is_none(),
        "owned native test must restore the original empty singleton"
    );
    result
}

// Characterization only: no policy repair or promised initial RED.
fn owned_characterization(delayed: bool, check: impl FnOnce(&Harness) -> Result<()>) -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ensure!(
        crate::mobile_manager::MobileApplicationManager::get_global_instance().is_none(),
        "OWNER_NOT_EMPTY"
    );
    let h = Harness::new_parallel(delayed).map_err(|_| anyhow::anyhow!("PARALLEL_SETUP"))?;
    let result = check(&h);
    let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(h);
        crate::mobile_manager::MobileApplicationManager::get_global_instance().is_none()
    }))
    .unwrap_or(false);
    if !cleanup {
        let _ = writeln!(
            std::io::stderr().lock(),
            "NATIVE_CHARACTERIZATION_CLEANUP_FAILED"
        );
    }
    result.and(cleanup.then_some(()).context("OWNED_CLEANUP_FAILED"))
}

fn accepted_context(state: &Value, pass: &MetaPasswordId, claim: &SsClaim) -> Result<Value> {
    let selected = secret(state, pass)?;
    ensure!(
        selected["primaryAction"] == "showRecovered",
        "SELECT_ACTION"
    );
    ensure!(selected["readiness"] == "ready", "SELECT_READINESS");
    let op = operation(state, claim)?;
    ensure!(op["clientStatus"] == "accepted", "A_STATUS");
    op.get("revealContext")
        .filter(|value| value.is_object())
        .cloned()
        .context("A_ABSENT")
}

fn checked_show(
    h: &Harness,
    selected: (&MetaPasswordId, &SsClaim, &str),
    a: &Value,
    changed: Option<&str>,
) -> Result<()> {
    let (pass, claim, plaintext) = selected;
    let current = projection().map_err(|_| anyhow::anyhow!("CURRENT_PROJECTION"))?;
    accepted_context(&current, pass, claim)?;
    let response: Value =
        serde_json::from_str(&crate::json_api::show_recovered(claim.id.wasm_id_str()))
            .map_err(|_| anyhow::anyhow!("SHOW_JSON"))?;
    ensure!(response["success"] == true, "SHOW_SUCCESS");
    ensure!(
        response["message"]["secret"].as_str() == Some(plaintext),
        "SHOW_EXACT_VALUE"
    );
    h.completion(claim)?;
    let r = response["message"]
        .get("revealReceipt")
        .filter(|v| v.is_object())
        .context("R_ABSENT")?;
    ensure!(r["version"] == 1 && a["version"] == 1, "SCHEMA");
    ensure!(r["secretId"] == pass.id_str(), "SELECTED_PASS");
    ensure!(
        r["recoveryClaimId"] == claim.id.wasm_id_str(),
        "SELECTED_CLAIM"
    );
    ensure!(
        r["requesterId"] == h.actor.device_id().to_string()
            && a["currentDeviceId"] == r["requesterId"],
        "REQUESTER"
    );
    let context = r
        .get("revealContext")
        .filter(|v| v.is_object())
        .context("R_CONTEXT_ABSENT")?;
    let after = projection().map_err(|_| anyhow::anyhow!("DONE_PROJECTION"))?;
    let done = operation(&after, claim)?;
    ensure!(done["clientStatus"] == "done", "D_STATUS");
    ensure!(
        secret(&after, pass)?["primaryAction"] == "recover",
        "D_ACTION"
    );
    let d = done
        .get("revealContext")
        .filter(|v| v.is_object())
        .context("D_CONTEXT_ABSENT")?;
    Harness::compare_context(context, d, None).context("R_D_COMPONENTS")?;
    ensure!(d == context, "R_D_CONTEXT");
    Harness::compare_context(a, context, changed)
}

#[test]
fn actual_native_json_parallel_secrets_preserve_receipt_operands() -> Result<()> {
    for delayed in [false, true] {
        for other_first in [true, false] {
            owned_characterization(delayed, |h| {
                let first = h.recovery.as_ref().context("FIRST_CLAIM")?;
                let other = h.parallel.as_ref().context("OTHER_CLAIM")?;
                ensure!(
                    h.secret != other.pass
                        && first.id != other.claim.id
                        && seed_fixture::VALUE != OTHER_VALUE,
                    "DISTINCT_INPUTS"
                );
                let before = projection().map_err(|_| anyhow::anyhow!("INITIAL_PROJECTION"))?;
                let a1 = accepted_context(&before, &h.secret, first)?;
                if delayed {
                    ensure!(
                        operation(&before, &other.claim)?["clientStatus"] == "pending",
                        "OTHER_PENDING"
                    );
                    h.canonical_delta(CanonicalDelta::ApproveOther)
                        .map_err(|_| anyhow::anyhow!("OTHER_APPROVAL"))?;
                }
                let ready = projection().map_err(|_| anyhow::anyhow!("APPROVED_PROJECTION"))?;
                let a2 = accepted_context(&ready, &other.pass, &other.claim)?;
                let first = (&h.secret, first, seed_fixture::VALUE);
                let second = (&other.pass, &other.claim, OTHER_VALUE);
                if other_first {
                    checked_show(h, second, &a2, None)?;
                    checked_show(h, first, &a1, None)?;
                } else {
                    checked_show(h, first, &a1, None)?;
                    checked_show(h, second, &a2, None)?;
                }
                Ok(())
            })?;
        }
    }
    Ok(())
}

#[test]
fn actual_native_json_unrelated_vault_revision_changes_context() -> Result<()> {
    owned_characterization(false, |h| {
        let claim = h.recovery.as_ref().context("FIRST_CLAIM")?;
        let before = projection().map_err(|_| anyhow::anyhow!("INITIAL_PROJECTION"))?;
        let a = accepted_context(&before, &h.secret, claim)?;
        h.canonical_delta(CanonicalDelta::VaultRevision)
            .map_err(|_| anyhow::anyhow!("VAULT_REVISION"))?;
        checked_show(
            h,
            (&h.secret, claim, seed_fixture::VALUE),
            &a,
            Some("vaultSource"),
        )
    })
}
