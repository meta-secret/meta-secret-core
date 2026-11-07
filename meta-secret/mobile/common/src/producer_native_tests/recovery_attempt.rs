//! Actual public JSON/SQLite/HTTP failure envelope; no UI E2E claim.
use super::{OWNER, harness::Harness};
use anyhow::{Result, ensure};
use meta_secret_core::node::{
    common::model::{secret::RecoveryClientStatus, user::common::UserDataMember},
    db::{
        descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
        events::kv_log_event::KvKey,
        objects::{
            persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
            persistent_vault::PersistentVault,
        },
        repo::generic_db::FindOneQuery,
    },
};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
#[test]
fn actual_presync_http_failure_returns_exact_bound_optional_attempt() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let claim = runtime.block_on(async {
        let vault = PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?;
        let claim = vault
            .0
            .value
            .to_vault_member(UserDataMember::from(h.target.user()))?
            .create_recovery_claim(h.secret.clone());
        ensure!(
            claim.compute_client_status(h.actor.device_id())
                == Some(RecoveryClientStatus::NeedApprove),
            "actual actor must be actionable"
        );
        let local = PersistentSharedSecret::from(h.p.clone());
        local.save_ss_log_event(claim.clone()).await?;
        // Use actual persisted log object, preserving its descriptor/body identity.
        let server =
            PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())));
        server.save_ss_log_event(claim.clone()).await?;
        Ok::<_, anyhow::Error>(claim)
    })?;
    h.http.transport_outage.store(true, Ordering::SeqCst);
    let actual: Value =
        serde_json::from_str(&crate::json_api::accept_recover(claim.id.wasm_id_str()))?;
    ensure!(
        actual["success"] == false && actual["error"].is_string(),
        "legacy failure must remain"
    );
    runtime.block_on(async {
        let keys = claim.recovery_db_ids();
        ensure!(
            !keys.is_empty(),
            "actual claim must identify physical response slots"
        );
        for recovery in keys {
            let key = KvKey::from(SsWorkflowDescriptor::Recovery(recovery));
            ensure!(
                h.p.repo.find_one(key.obj_id).await?.is_none(),
                "actual pre-sync failure must create zero exact Recovery source"
            );
        }
        Ok::<_, anyhow::Error>(())
    })?;
    assert_eq!(
        actual["decisionAttempt"],
        json!({
            "schemaVersion":1,"kind":"recoverApprove","claimId":claim.id,
            "actorId":h.actor.device_id(),"vaultName":h.actor.vault_name,
            "phase":"notSubmitted"
        }),
        "semantic RED: actual legacy failure lacks narrowly bound optional evidence"
    );
    assert_eq!(
        actual.as_object().unwrap().len(),
        3,
        "exact additive envelope only"
    );
    Ok(())
}
#[test]
fn actual_queued_response_survives_reopened_sqlite_and_never_grants_attempt_proof() -> Result<()> {
    use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
    use meta_secret_core::node::{
        app::orchestrator::MetaOrchestrator,
        db::{
            descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
            events::kv_log_event::KvKey,
        },
    };
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for decline in [false, true] {
        let h = Harness::new()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (claim, sources) = runtime.block_on(async {
            let vault = PersistentVault::from(h.p.clone())
                .get_vault(h.actor.vault_name.clone())
                .await?;
            let claim = vault
                .0
                .value
                .to_vault_member(UserDataMember::from(h.target.user()))?
                .create_recovery_claim(h.secret.clone());
            ensure!(
                claim.compute_client_status(h.actor.device_id())
                    == Some(RecoveryClientStatus::NeedApprove),
                "real pending receiver required"
            );
            PersistentSharedSecret::from(h.p.clone())
                .save_ss_log_event(claim.clone())
                .await?;
            let executor = MetaOrchestrator {
                p_obj: h.p.clone(),
                user_creds: h.actor.clone(),
            };
            if decline {
                executor.decline_recover(claim.id.clone()).await?;
            } else {
                executor.accept_recover(claim.id.clone()).await?;
            }
            let reopened = SqlIteRepo {
                conn_url: h.p.repo.conn_url.clone(),
            };
            let mut sources = Vec::new();
            for id in claim.recovery_db_ids() {
                for descriptor in [
                    SsWorkflowDescriptor::Recovery(id.clone()),
                    SsWorkflowDescriptor::Decline(id),
                ] {
                    let key = KvKey::from(descriptor).obj_id;
                    let actual = h.p.repo.find_one(key.clone()).await?;
                    ensure!(
                        reopened.find_one(key.clone()).await? == actual,
                        "reopened physical SQLite must preserve exact source identity/body"
                    );
                    if let Some(event) = actual {
                        sources.push((key, event));
                    }
                }
            }
            ensure!(
                !sources.is_empty(),
                "actual response executor must persist source"
            );
            Ok::<_, anyhow::Error>((claim, sources))
        })?;
        h.http.transport_outage.store(true, Ordering::SeqCst);
        let actual: Value =
            serde_json::from_str(&crate::json_api::accept_recover(claim.id.wasm_id_str()))?;
        ensure!(
            actual["success"] == false && actual["error"].is_string(),
            "original failure retained"
        );
        ensure!(
            actual.get("decisionAttempt").is_none(),
            "prior actual response must veto proof"
        );
        ensure!(
            actual.as_object().unwrap().len() == 2,
            "legacy envelope only when unproved"
        );
        runtime.block_on(async {
            let reopened = SqlIteRepo {
                conn_url: h.p.repo.conn_url.clone(),
            };
            for (key, event) in sources {
                ensure!(
                    reopened.find_one(key).await? == Some(event),
                    "failed invocation must not delete/remint/replace prior actual response"
                );
            }
            Ok::<_, anyhow::Error>(())
        })?;
    }
    Ok(())
}
#[test]
fn actual_incomplete_or_foreign_status_maps_never_grant_attempt_proof() -> Result<()> {
    use meta_secret_core::node::common::model::secret::SsDistributionStatus;
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for fault in [
        "empty",
        "missing-own",
        "missing-other",
        "extra-sender",
        "duplicate-receiver",
        "own-sent",
        "other-sent",
    ] {
        let h = Harness::new()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let claim = runtime.block_on(async {
            let vault = PersistentVault::from(h.p.clone())
                .get_vault(h.actor.vault_name.clone())
                .await?;
            let mut claim = vault
                .0
                .value
                .to_vault_member(UserDataMember::from(h.target.user()))?
                .create_recovery_claim(h.secret.clone());
            ensure!(
                claim.receivers.len() == 2 && claim.status.statuses.len() == 2,
                "real original three-member complete pending map required"
            );
            match fault {
                "empty" => claim.status.statuses.clear(),
                "missing-own" => {
                    claim.status.statuses.remove(h.actor.device_id());
                }
                "missing-other" => {
                    let other = claim
                        .receivers
                        .iter()
                        .find(|id| *id != h.actor.device_id())
                        .expect("actual other receiver")
                        .clone();
                    claim.status.statuses.remove(&other);
                }
                "extra-sender" => {
                    claim
                        .status
                        .statuses
                        .insert(claim.sender.clone(), SsDistributionStatus::Pending);
                }
                "duplicate-receiver" => claim.receivers.push(h.actor.device_id().clone()),
                "own-sent" => {
                    claim
                        .status
                        .statuses
                        .insert(h.actor.device_id().clone(), SsDistributionStatus::Sent);
                }
                "other-sent" => {
                    let other = claim
                        .receivers
                        .iter()
                        .find(|id| *id != h.actor.device_id())
                        .expect("actual other receiver")
                        .clone();
                    claim
                        .status
                        .statuses
                        .insert(other, SsDistributionStatus::Sent);
                }
                _ => unreachable!(),
            }
            PersistentSharedSecret::from(h.p.clone())
                .save_ss_log_event(claim.clone())
                .await?;
            Ok::<_, anyhow::Error>(claim)
        })?;
        h.http.transport_outage.store(true, Ordering::SeqCst);
        let actual: Value =
            serde_json::from_str(&crate::json_api::accept_recover(claim.id.wasm_id_str()))?;
        ensure!(
            actual["success"] == false && actual["error"].is_string(),
            "legacy failure retained"
        );
        assert!(
            actual.get("decisionAttempt").is_none(),
            "malformed complete-pending-map evidence must refuse, case={fault}"
        );
        runtime.block_on(async {
            for source in claim.recovery_db_ids() {
                let key = KvKey::from(SsWorkflowDescriptor::Recovery(source));
                ensure!(
                    h.p.repo.find_one(key.obj_id).await?.is_none(),
                    "malformed evidence must never dispatch approval"
                );
            }
            Ok::<_, anyhow::Error>(())
        })?;
    }
    Ok(())
}
#[test]
fn healthy_public_json_uses_actual_fifo_and_preserves_exact_legacy_success() -> Result<()> {
    use meta_secret_core::node::common::model::secret::SsDistributionStatus;
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let claim = runtime.block_on(async {
        let vault = PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?;
        let claim = vault
            .0
            .value
            .to_vault_member(UserDataMember::from(h.target.user()))?
            .create_recovery_claim(h.secret.clone());
        PersistentSharedSecret::from(h.p.clone())
            .save_ss_log_event(claim.clone())
            .await?;
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())))
            .save_ss_log_event(claim.clone())
            .await?;
        Ok::<_, anyhow::Error>(claim)
    })?;
    // Existing singleton service.run supplies the real queued GetState response.
    let actual: Value =
        serde_json::from_str(&crate::json_api::accept_recover(claim.id.wasm_id_str()))?;
    assert_eq!(
        actual,
        json!({"success":true}),
        "legacy exact success envelope must remain"
    );
    runtime.block_on(async {
        let local = PersistentSharedSecret::from(h.p.clone())
            .get_ss_log_obj(h.actor.vault_name.clone())
            .await?;
        let remote =
            PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())))
                .get_ss_log_obj(h.actor.vault_name.clone())
                .await?;
        for log in [&local, &remote] {
            let returned = log
                .claims
                .get(&claim.id)
                .expect("actual exact completed claim");
            ensure!(
                returned.status.statuses.get(h.actor.device_id())
                    == Some(&SsDistributionStatus::Sent),
                "actual server and local canonical status must contain the submitted response"
            );
        }
        for source in claim.recovery_db_ids() {
            let key = KvKey::from(SsWorkflowDescriptor::Recovery(source));
            ensure!(
                h.p.repo.find_one(key.obj_id).await?.is_none(),
                "successful actual post-sync must clear acknowledged local source"
            );
        }
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}
#[test]
fn legacy_unknown_opaque_claim_and_uninitialized_json_never_gain_attempt_metadata() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    {
        let _h = Harness::new()?;
        for id in ["", "{malformed-opaque-id"] {
            let actual: Value = serde_json::from_str(&crate::json_api::accept_recover(id.into()))?;
            ensure!(
                actual["success"] == false && actual["error"].is_string(),
                "legacy error retained"
            );
            ensure!(
                actual.as_object().unwrap().len() == 2 && actual.get("decisionAttempt").is_none(),
                "unknown opaque claim must not gain evidence"
            );
        }
    }
    ensure!(
        super::super::super::MobileApplicationManager::get_global_instance().is_none(),
        "fixture must restore absent singleton"
    );
    let absent: Value = serde_json::from_str(&crate::json_api::accept_recover("absent".into()))?;
    assert_eq!(
        absent,
        json!({"success":false,"error":"Accept recover request is failed"})
    );
    Ok(())
}
fn check_receiver_terminal_json_case(decision: &str) -> Result<()> {
    use meta_secret_core::node::{
        api::{SyncRequest, WriteSyncRequest},
        app::{orchestrator::MetaOrchestrator, sync::sync_protocol::SyncProtocol},
        common::model::IdString,
        security::sign_event,
    };
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let claim = runtime.block_on(async {
        let vault = PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?;
        let claim = vault
            .0
            .value
            .to_vault_member(UserDataMember::from(h.target.user()))?
            .create_recovery_claim(h.secret.clone());
        ensure!(
            claim.receivers.contains(h.actor.device_id())
                && claim.receivers.contains(h.initiator.device_id()),
            "actual two receivers required"
        );
        PersistentSharedSecret::from(h.p.clone())
            .save_ss_log_event(claim.clone())
            .await?;
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())))
            .save_ss_log_event(claim.clone())
            .await?;
        Ok::<_, anyhow::Error>(claim)
    })?;
    if decision == "own-approve" || decision == "own-decline" {
        let response = if decision == "own-approve" {
            crate::json_api::accept_recover(claim.id.wasm_id_str())
        } else {
            crate::json_api::decline_recover(claim.id.wasm_id_str())
        };
        let actual: Value = serde_json::from_str(&response)?;
        ensure!(
            actual == json!({"success":true}),
            "actual native own decision required"
        );
    } else {
        runtime.block_on(async {
            use meta_secret_core::node::{
                common::model::secret::SsDistributionId,
                db::{
                    descriptors::{
                        shared_secret_descriptor::SsLogDescriptor,
                        vault_descriptor::VaultStatusDescriptor,
                    },
                    events::{
                        generic_log_event::ObjIdExtractor,
                        object_id::{ArtifactId, Next},
                    },
                    in_mem_db::InMemKvLogEventRepo,
                    repo::generic_db::SaveCommand,
                },
            };
            let peer_p = Arc::new(PersistentObject::new(Arc::new(
                InMemKvLogEventRepo::default(),
            )));
            // Whitelisted controlled fixture records only; no actor credentials or synthetic result.
            let vault = PersistentVault::from(h.p.clone())
                .get_vault(h.actor.vault_name.clone())
                .await?;
            peer_p.repo.save(vault).await?;
            for user in [&h.actor, &h.initiator, &h.target] {
                let status =
                    h.p.find_tail_event(VaultStatusDescriptor::from(user.user_id()))
                        .await?
                        .expect("actual controlled member status");
                peer_p.repo.save(status).await?;
            }
            let actor_before = PersistentSharedSecret::from(h.p.clone())
                .find_ss_log_tail_event(h.actor.vault_name.clone())
                .await?
                .expect("actual pending log");
            ensure!(
                !actor_before
                    .0
                    .value
                    .liveboard_recovery_results
                    .contains_key(&claim.id),
                "isolated peer must start from real pending claim without a result"
            );
            let descriptor = SsLogDescriptor::from(h.actor.vault_name.clone());
            let history =
                h.p.get_object_events_from_beginning(descriptor.clone())
                    .await?;
            ensure!(
                !history.is_empty() && history.last() == Some(&actor_before),
                "actual complete contiguous history must end at actor tail"
            );
            let mut expected = ArtifactId::from(descriptor);
            for row in history {
                ensure!(
                    row.obj_id() == expected,
                    "actual SsLog rows must be contiguous from first key"
                );
                expected = expected.next();
                peer_p.repo.save(row).await?;
            }
            let peer_before = PersistentSharedSecret::from(peer_p.clone())
                .get_ss_log_obj(h.actor.vault_name.clone())
                .await?;
            ensure!(
                peer_before == actor_before.0.value && peer_before.claims.contains_key(&claim.id),
                "actual reopened peer state must equal actor pending state before producer"
            );
            for user in [&h.actor, &h.initiator, &h.target] {
                let distribution =
                    h.p.find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                        pass_id: h.secret.clone(),
                        receiver: user.device_id().clone(),
                    }))
                    .await?
                    .expect("actual controlled encrypted distribution");
                peer_p.repo.save(distribution).await?;
            }
            let peer = MetaOrchestrator {
                p_obj: peer_p.clone(),
                user_creds: h.initiator.clone(),
            };
            if decision == "peer-approve" {
                peer.accept_recover(claim.id.clone()).await?;
            } else {
                peer.decline_recover(claim.id.clone()).await?;
            }
            let peer_id = claim
                .recovery_db_ids()
                .into_iter()
                .find(|id| id.distribution_id.receiver == *h.initiator.device_id())
                .expect("exact real peer response slot");
            let descriptor = if decision == "peer-approve" {
                SsWorkflowDescriptor::Recovery(peer_id)
            } else {
                SsWorkflowDescriptor::Decline(peer_id)
            };
            let key = KvKey::from(descriptor).obj_id;
            let source = peer_p
                .repo
                .find_one(key)
                .await?
                .expect("actual isolated peer-produced source");
            let actor_after = PersistentSharedSecret::from(h.p.clone())
                .find_ss_log_tail_event(h.actor.vault_name.clone())
                .await?
                .expect("actual actor tail");
            ensure!(
                actor_after == actor_before,
                "peer producer must never mutate actor database"
            );
            let action = sign_event(
                source,
                h.initiator.device_id().clone(),
                &h.initiator.device_creds.key_manager()?.dsa,
            )?;
            let manager = super::super::super::MobileApplicationManager::get_global_instance()
                .expect("actual native singleton");
            manager
                .app_manager
                .server
                .send(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                    action,
                ))))
                .await?
                .ensure_success()?;
            Ok::<_, anyhow::Error>(())
        })?;
    }
    let state: Value = serde_json::from_str(&crate::json_api::get_state())?;
    ensure!(
        state["success"] == true,
        "actual successful native canonical read required"
    );
    let board = state
        .pointer("/message/state/vault/member/liveboardState")
        .expect("actual native liveboard projection");
    ensure!(
        board["currentDeviceId"] == json!(h.actor.device_id())
            && board["vaultName"] == json!(h.actor.vault_name),
        "receiver scope must match"
    );
    let found: Vec<_> = board["operations"]
        .as_array()
        .expect("actual operations")
        .iter()
        .filter(|op| op["kind"] == "recover" && op["requestId"] == json!(claim.id))
        .collect();
    let [operation] = found.as_slice() else {
        anyhow::bail!("exact unique actual Recover operation required")
    };
    let expected_responder = if decision == "own-decline" {
        Value::Null
    } else if decision == "own-approve" {
        json!(h.actor.device_id())
    } else {
        json!(h.initiator.device_id())
    };
    let expected_outcome = if decision == "own-decline" {
        Value::Null
    } else if decision == "peer-decline" {
        json!("declined")
    } else {
        json!("accepted")
    };
    let expected_resolution = if decision == "own-decline" {
        "unknown"
    } else {
        "verified"
    };
    let expected_status = if decision == "peer-approve" {
        "needApprove"
    } else {
        "unknown"
    };
    // Only public identity/permission/result metadata; never dump full state or secret material.
    println!(
        "RECOVERY_RECEIVER_TERMINAL_TUPLE {}",
        json!({
            "case":decision,"kind":operation["kind"],"requestId":operation["requestId"],
            "requesterId":operation["requesterId"],"secretId":operation["secretId"],
            "outcome":operation["outcome"],"responderId":operation["responderId"],
            "resolutionSource":operation["resolutionSource"],"readiness":operation["readiness"],
            "clientStatus":operation["clientStatus"],"canApprove":operation["canApprove"],
            "canDecline":operation["canDecline"]})
    );
    runtime.block_on(async {
            use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
            let local = PersistentSharedSecret::from(h.p.clone())
                .find_ss_log_tail_event(h.actor.vault_name.clone()).await?.expect("actual local tail");
            let server = PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())))
                .find_ss_log_tail_event(h.actor.vault_name.clone()).await?.expect("actual server tail");
            let same_scope = local.obj_id().fqdn == server.obj_id().fqdn;
            for (location, log) in [("local", local), ("server", server)] {
                let record = log.0.value.liveboard_recovery_results.get(&claim.id);
                let selected = log.0.value.claims.get(&claim.id);
                println!("RECOVERY_RECEIVER_CANONICAL_METADATA {}", json!({
                    "case":decision,"location":location,"sameScope":same_scope,
                    "tailRevision":log.obj_id().id.curr,"claimPresent":selected.is_some(),
                    "actorStatus":selected.and_then(|c| c.status.statuses.get(h.actor.device_id())),
                    "peerStatus":selected.and_then(|c| c.status.statuses.get(h.initiator.device_id())),
                    "verifiedRecordPresent":record.is_some(),
                    "recordOutcome":record.map(|r| r.outcome),
                    "recordResponder":record.map(|r| &r.responder_id),
                    "sourceProofPresent":record.is_some_and(|r| r.source.is_some())}));
            }
            Ok::<_, anyhow::Error>(())
        })?;

    ensure!(
        operation["requesterId"] == json!(h.target.device_id())
            && operation["secretId"] == json!(h.secret.id.clone().id_str())
            && operation["outcome"] == expected_outcome
            && operation["responderId"] == expected_responder
            && operation["resolutionSource"] == expected_resolution
            && operation["readiness"] == "waiting"
            && operation["clientStatus"] == expected_status
            && operation["canApprove"] == false
            && operation["canDecline"] == false,
        "actual receiver tuple must retain exact identity, qualified resolution and disabled permissions"
    );
    if decision == "own-decline" {
        ensure!(
            operation["outcome"].is_null()
                && operation["responderId"].is_null()
                && operation["resolutionSource"] != "verified",
            "observed unverified negative must not be treated as verified terminal or retry permission"
        );
    }
    Ok(())
}

#[test]
fn actual_receiver_terminal_json_tuples_bind_identity_and_permissions_after_real_decisions()
-> Result<()> {
    check_receiver_terminal_json_case("own-approve")
}
#[test]
fn actual_receiver_terminal_json_own_decline_is_unverified_negative() -> Result<()> {
    check_receiver_terminal_json_case("own-decline")
}
#[test]
fn actual_receiver_terminal_json_peer_approve_preserves_verified_oracle() -> Result<()> {
    check_receiver_terminal_json_case("peer-approve")
}
#[test]
fn actual_receiver_terminal_json_peer_decline_preserves_verified_oracle() -> Result<()> {
    check_receiver_terminal_json_case("peer-decline")
}
