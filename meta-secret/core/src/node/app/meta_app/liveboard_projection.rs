//! Allowlisted Liveboard snapshot, computed after the existing client finalization.
use super::{liveboard_material::Material, liveboard_operations::operations};
use crate::node::common::model::{
    liveboard::{LiveboardOperationKind, LiveboardSecretState, LiveboardState},
    secret::{RecoveryClientStatus, SsLogData},
    user::user_creds::UserCreds,
    vault::vault::VaultMember,
};
use crate::node::db::{
    actions::recovery_reveal::{RevealEvidence, RevealInputs, try_reveal_context},
    events::vault::vault_event::VaultObject,
    events::vault::vault_log_event::VaultActionEvents,
    objects::{
        persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
    },
    repo::generic_db::KvLogEventRepo,
};
use anyhow::Result;
use std::sync::Arc;

pub(super) async fn project<R: KvLogEventRepo>(
    p_obj: Arc<PersistentObject<R>>,
    creds: &UserCreds,
    member: &VaultMember,
    vault_source: &VaultObject,
    events: &VaultActionEvents,
    claims: &SsLogData,
) -> Result<LiveboardState> {
    let vault = &member.vault;
    vault.ensure_valid_member_count()?;
    let current = creds.device_id();
    let material = Material {
        p_obj: p_obj.clone(),
        creds,
        member,
        claims,
    };
    let mut ops = operations(member, events, claims);
    let mut secrets = vec![];
    for pass in &vault.secrets {
        let deletion = ops
            .iter()
            .find(|op| {
                op.kind == LiveboardOperationKind::DeleteSecret
                    && op.secret_id.as_deref() == Some(&pass.id_str())
                    && op.outcome.is_none()
            })
            .map(|op| op.request_id.clone());
        let mut state = LiveboardSecretState {
            secret_id: pass.id_str(),
            primary_action: "unavailable".into(),
            recovery_claim_id: None,
            deletion_request_id: deletion.clone(),
            can_delete: deletion.is_none(),
            readiness: "synchronizing".into(),
        };
        let local = material.local_material(pass).await.unwrap_or(None);
        let mut accepted_context = None;
        if vault.ensure_local_secret_available(current, pass).is_ok() && local.is_some() {
            state.primary_action = "showLocal".into();
            state.readiness = "ready".into();
        } else if let Ok(active) = claims.find_unique_active_recovery_claim_id(current, pass) {
            if let Some(id) = active {
                let Some(claim) = claims.claims.get(&id) else {
                    secrets.push(state);
                    continue;
                };
                state.recovery_claim_id = Some(id.wasm_id_str());
                if claim.client_status == Some(RecoveryClientStatus::Accepted) {
                    let recoveries = PersistentSharedSecret::from(p_obj.clone())
                        .get_recoveries(claim.clone())
                        .await
                        .unwrap_or_default();
                    if let Some(decoded) = material
                        .recovered_material(
                            claim,
                            local.as_ref().map(|value| &value.share),
                            &recoveries,
                        )
                        .await
                        .unwrap_or(None)
                    {
                        state.primary_action = "showRecovered".into();
                        state.readiness = "ready".into();
                        accepted_context = try_reveal_context(RevealInputs {
                            vault: vault_source,
                            creds,
                            claims,
                            claim,
                            local: local.as_ref().map(|value| &value.source),
                            recoveries: &recoveries,
                            evidence: RevealEvidence::VerifiedDecoded(&decoded),
                        });
                    }
                } else {
                    state.primary_action = "waiting".into();
                    state.readiness = "waiting".into();
                }
            } else if local.is_some() {
                state.primary_action = "recover".into();
                state.readiness = "ready".into();
            }
        }
        for op in ops.iter_mut().filter(|op| {
            op.kind == LiveboardOperationKind::Recover
                && op.secret_id.as_deref() == Some(&state.secret_id)
                && state.recovery_claim_id.as_deref() == Some(&op.request_id)
        }) {
            op.readiness = state.readiness.clone();
            op.reveal_context = accepted_context.clone();
        }
        if state.primary_action == "recover" && state.recovery_claim_id.is_none() {
            for op in ops.iter_mut().filter(|op| {
                op.kind == LiveboardOperationKind::Recover
                    && op.secret_id.as_deref() == Some(&state.secret_id)
                    && op.requester_id == current.to_string()
                    && op.client_status == "done"
            }) {
                let Some(claim) = claims
                    .claims
                    .values()
                    .find(|claim| claim.id.wasm_id_str() == op.request_id)
                else {
                    continue;
                };
                let Ok(recoveries) = PersistentSharedSecret::from(p_obj.clone())
                    .get_recoveries(claim.clone())
                    .await
                else {
                    continue;
                };
                let Some(local) = &local else { continue };
                op.reveal_context = try_reveal_context(RevealInputs {
                    vault: vault_source,
                    creds,
                    claims,
                    claim,
                    local: Some(&local.source),
                    recoveries: &recoveries,
                    evidence: RevealEvidence::DoneSourceContinuity {
                        local: &local.share,
                    },
                });
            }
        }
        secrets.push(state);
    }
    secrets.sort_by(|a, b| a.secret_id.cmp(&b.secret_id));
    let mut removable: Vec<_> = vault
        .members()
        .iter()
        .filter(|m| m.user_data.device.device_id != *current)
        .map(|m| m.user_data.device.device_id.to_string())
        .collect();
    removable.sort();
    Ok(LiveboardState {
        schema_version: 1,
        vault_name: vault.vault_name.0.clone(),
        current_device_id: current.to_string(),
        storage_mode: match vault.members().len() {
            1 => "whole",
            2 => "copies",
            _ => "shares",
        }
        .into(),
        can_create_secret: vault.is_member(current),
        removable_device_ids: removable,
        secrets,
        operations: ops,
    })
}
