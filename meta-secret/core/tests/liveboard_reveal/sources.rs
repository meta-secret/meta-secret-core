use super::fixture::Recovery;
use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    common::model::{
        secret::{SecretDistributionType, SsDistributionStatus},
        user::common::UserDataMember,
    },
    db::{
        events::{generic_log_event::*, object_id::Next, shared_secret_event::SsWorkflowObject},
        objects::persistent_vault::PersistentVault,
        repo::generic_db::*,
    },
};

#[tokio::test]
async fn complete_ciphertext_and_vault_keys_are_committed_with_separate_domains() -> Result<()> {
    let r = Recovery::new().await?;
    let context = r.context().await?.context("real accepted proof")?;
    let source = r.source().await?;
    let vault = PersistentVault::from(r.h.p.clone())
        .get_vault(r.h.vault.vault_name.clone())
        .await?;
    let local = r.local().await?;
    for (field, kind, body) in [
        (
            &context["vaultSource"],
            "Vault",
            meta_secret_core::node::security::canonical_json(&vault)?,
        ),
        (
            &context["localDistributionSource"],
            "Distribution",
            meta_secret_core::node::security::canonical_json(&local)?,
        ),
        (
            &context["recoverySources"][0],
            "Recovery",
            meta_secret_core::node::security::canonical_json(&source)?,
        ),
    ] {
        let expected = meta_secret_core::crypto::utils::sha256_hex(&format!(
            "MetaSecret.LiveboardReveal.{kind}.v1\n{}",
            String::from_utf8(body)?
        ));
        ensure!(
            field["bodyCommitment"] == expected,
            "complete typed source body commitment"
        );
    }
    ensure!(
        context["recoverySources"][0]["eventId"] == serde_json::to_value(source.obj_id())?,
        "exact physical source key"
    );
    Ok(())
}

#[tokio::test]
async fn fixed_key_reencrypted_body_and_exact_vault_revision_change_context() -> Result<()> {
    let r = Recovery::new().await?;
    let old = r.context().await?.context("old")?;
    let original = r.source().await?;
    let replacement = r.packet(&r.h.initiator, &r.h.initiator).await?;
    ensure!(
        original.obj_id() == replacement.obj_id() && original != replacement,
        "same fixed key new ciphertext"
    );
    ensure!(
        r.context().await?.is_some_and(|new| new != old),
        "body under same ID cannot retain old context"
    );
    r.h.p.repo.replace(original).await?;
    let mut vault = PersistentVault::from(r.h.p.clone())
        .get_vault(r.h.vault.vault_name.clone())
        .await?;
    vault.0.key = vault.0.key.next();
    r.h.p.repo.save(vault).await?;
    ensure!(
        r.context().await?.is_some_and(|new| new != old),
        "same full Vault body at next source identity differs"
    );
    Ok(())
}

#[tokio::test]
async fn new_active_or_already_done_claim_and_material_aba_cannot_match_old_scope() -> Result<()> {
    for terminal in [false, true] {
        let r = Recovery::new().await?;
        let old = r.context().await?.context("old")?;
        let mut other =
            r.h.vault
                .clone()
                .to_vault_member(UserDataMember::from(r.h.approver.user()))?
                .create_recovery_claim(r.pass.clone());
        if terminal {
            other.status.statuses.insert(
                r.h.approver.device_id().clone(),
                SsDistributionStatus::Delivered,
            );
        }
        r.save_claim(&other).await?;
        ensure!(
            r.context().await?.is_none_or(|new| new != old),
            "new Claim including already-Done changes or removes context"
        );
    }
    let r = Recovery::new().await?;
    let old = r.context().await?.context("old")?;
    let mut split =
        r.h.vault
            .clone()
            .to_vault_member(UserDataMember::from(r.h.approver.user()))?
            .create_split_claim(r.pass.clone());
    for status in split.status.statuses.values_mut() {
        *status = SsDistributionStatus::Delivered;
    }
    ensure!(
        split.distribution_type == SecretDistributionType::Split,
        "new material generation"
    );
    r.save_claim(&split).await?;
    let original = r.local().await?;
    let mut replacement = original.clone();
    if let SsWorkflowObject::Distribution(event) = &mut replacement {
        event.value.claim_id = split.dist_claim_id.clone();
    }
    r.h.p.repo.replace(replacement).await?;
    // A provisional generation can also be rejected by the existing finalizer;
    // it must never expose the old usable source context.
    let _ = r.context().await;
    r.h.p.repo.replace(original).await?;
    ensure!(
        r.context()
            .await
            .ok()
            .flatten()
            .is_none_or(|new| new != old),
        "returning old material cannot erase canonical new Split fence"
    );
    split.status.statuses.clear();
    r.save_claim(&split).await?;
    ensure!(
        r.context()
            .await
            .ok()
            .flatten()
            .is_none_or(|new| new != old),
        "non-target statuses retained in scope"
    );
    Ok(())
}

#[tokio::test]
async fn missing_wrong_claim_vault_channel_and_nonallowed_status_have_no_original_proof()
-> Result<()> {
    for variant in 0..5 {
        let r = Recovery::new().await?;
        let old = r.context().await?.context("old")?;
        let mut packet = r.source().await?;
        match variant {
            0 => {
                r.h.p.repo.try_delete(packet.obj_id()).await?;
            }
            1 => {
                if let SsWorkflowObject::Recovery(event) = &mut packet {
                    event.value.vault_name.0 = "foreign".into();
                }
                r.h.p.repo.replace(packet).await?;
            }
            2 => {
                r.packet(&r.h.initiator, &r.h.target).await?;
            }
            3 => {
                let mut claim = r.claim.clone();
                claim.status.statuses.insert(
                    r.h.approver.device_id().clone(),
                    SsDistributionStatus::Pending,
                );
                r.save_claim(&claim).await?;
            }
            _ => {
                let mut claim = r.claim.clone();
                claim.receivers.reverse();
                r.save_claim(&claim).await?;
            }
        }
        ensure!(
            r.context().await?.is_none_or(|new| new != old),
            "invalid or changed canonical source cannot retain original proof"
        );
    }
    Ok(())
}

use meta_secret_core::node::db::{
    actions::recover::RecoveryHandler, events::vault::vault_event::VaultObject,
};
use serde_json::Value;

pub(super) async fn vault_context(
    r: &Recovery,
    vault: &VaultObject,
    recover: bool,
) -> Result<Option<Value>> {
    let view =
        serde_json::to_value(vault).map_err(|_| anyhow::anyhow!("SOURCE_VIEW_SETUP_HOLD"))?;
    r.h.p
        .repo
        .replace(vault.clone())
        .await
        .map_err(|_| anyhow::anyhow!("SOURCE_REPLACE_SETUP_HOLD"))?;
    let mut context = None;
    for before_context in [true, false] {
        let actual = PersistentVault::from(r.h.p.clone())
            .get_vault(r.h.vault.vault_name.clone())
            .await
            .map_err(|_| anyhow::anyhow!("SOURCE_READBACK_SETUP_HOLD"))?;
        let actual_view =
            serde_json::to_value(&actual).map_err(|_| anyhow::anyhow!("SOURCE_VIEW_SETUP_HOLD"))?;
        ensure!(
            actual == *vault && actual.obj_id() == vault.obj_id() && actual_view == view,
            "EXACT_SOURCE_SETUP_HOLD"
        );
        if before_context && recover {
            let result = RecoveryHandler {
                p_obj: r.h.p.clone(),
            }
            .recover_by_claim_id_checked(r.h.approver.clone(), r.claim.id.clone())
            .await
            .map_err(|_| anyhow::anyhow!("CHECKED_RECOVERY_SETUP_HOLD"))?;
            ensure!(
                result.plaintext.text == super::fixtures::VALUE,
                "EXACT_TARGET_VALUE_SETUP_HOLD"
            );
            let receipt = result
                .reveal_receipt
                .context("CHECKED_RECEIPT_SETUP_HOLD")?;
            ensure!(
                receipt.is_valid()
                    && receipt.version == 1
                    && receipt.secret_id == r.pass.id_str()
                    && receipt.recovery_claim_id == r.claim.id.wasm_id_str()
                    && receipt.requester_id == r.h.approver.device_id().to_string(),
                "EXACT_RECEIPT_IDENTITY_SETUP_HOLD"
            );
            context = Some(
                serde_json::to_value(receipt.reveal_context)
                    .map_err(|_| anyhow::anyhow!("CHECKED_CONTEXT_SETUP_HOLD"))?,
            );
        } else if before_context {
            let state = r.state().await.map_err(|_| anyhow::anyhow!("STATE"))?;
            let ops = state["operations"].as_array().context("OPERATIONS")?;
            let op = ops
                .iter()
                .find(|op| op["requestId"] == r.claim.id.wasm_id_str())
                .context("SELECTED_OPERATION")?;
            context = op.get("revealContext").cloned();
        }
    }
    if let Some(context) = &context {
        ensure!(
            view["key"]["objId"].is_object()
                && context["vaultSource"]["eventId"] == view["key"]["objId"],
            "CONSUMED_SOURCE_SETUP_HOLD"
        );
    }
    Ok(context)
}
