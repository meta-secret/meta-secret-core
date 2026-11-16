//! No-fault checked2→1 must actually reach its terminal bookkeeping boundary.
use super::{context_repo::Repo, fixture::*};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::{liveboard_producer::*, secret::SsDistributionId},
    db::{
        descriptors::{
            local_removal_envelope::LocalRemovalEnvelopeDescriptor,
            shared_secret_descriptor::{SsLogDescriptor, SsWorkflowDescriptor},
            vault_descriptor::{VaultDescriptor, VaultLogDescriptor},
        },
        events::generic_log_event::*,
        in_mem_db::InMemKvLogEventRepo,
        objects::persistent_object::PersistentObject,
        repo::generic_db::*,
    },
};
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn healthy_checked_local2to1_reaches_applied_ready_and_clears_actual_context() -> Result<()> {
    let inner = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Harness::new(inner, 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        let receipt = h.remove(false).await?;
        let local = h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
            .await?.context("actual no-fault local Vault")?;
        let remote = Arc::new(PersistentObject::new(h.network.repo.clone()));
        let canonical = remote.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
            .await?.context("actual no-fault remote Vault")?;
        let current = h.p.find_tail_event(LocalRemovalEnvelopeDescriptor(h.actor.user_id()))
            .await?.and_then(|event| event.0.value.producer_context);
        let sources = h.p.repo.sources();
        // Opaque lifecycle status/counts only: no body, signature, keys or material are logged.
        eprintln!("HEALTHY_CHECKED phase={:?} canonical={:?} material={:?} readiness={} local_members={} remote_members={} active_context={} source_attempts={}",
            receipt.phase, receipt.canonical_state, receipt.material_stage, receipt.readiness,
            local.0.value.members().len(), canonical.0.value.members().len(), current.is_some(), sources.len());
        let split = h.p.find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: h.primary.clone(), receiver: h.actor.device_id().clone(),
        })).await?.context("actual retained own ciphertext")?.to_distribution_data()?;
        let claims = h.p.find_tail_event(SsLogDescriptor::from(h.actor.vault_name.clone()))
            .await?.context("actual local canonical split claims")?;
        let claim = claims.0.value.claims.get(&split.claim_id.id).context("actual ciphertext claim")?;
        let ciphertext = split.secret_message.cipher_text();
        let clear = ciphertext.decrypt(&h.actor.device_creds.secret_box.transport.sk)?;
        let share = meta_secret_core::secret::shared_secret::UserShareDto::try_from(&clear.msg)?;
        let cfg = share.share_blocks.first().context("actual share shape")?.config;
        eprintln!("HEALTHY_MATERIAL ciphertext_sender_current={} claim_sender_current={} singleton_claim={} stored_n={} stored_k={} current_n={} current_k={}",
            local.0.value.is_member(&ciphertext.channel.sender().to_device_id()),
            claim.sender == *h.actor.device_id(), claim.receivers.is_empty(),
            cfg.number_of_shares, cfg.threshold, local.0.value.sss_cfg().number_of_shares,
            local.0.value.sss_cfg().threshold);
        let before_show_writes = h.p.repo.inner.writes.load(Ordering::SeqCst);
        let before_show_network = h.network.counts();
        for pass in [&h.primary, &h.secondary] {
            let actual = h.web.show_local_secret(pass.clone()).await?;
            assert!(actual.text == super::seed_fixture::VALUE,
                "existing public ShowLocal must return the known exact fixture value");
        }
        assert_eq!(h.p.repo.inner.writes.load(Ordering::SeqCst), before_show_writes);
        assert_eq!(h.network.counts(), before_show_network);
        eprintln!("HEALTHY_SHOW_LOCAL exact_secret_count=2 success=true local_writes=0 network_requests=0");
        assert_eq!(sources.len(), 2);
        assert_source(&receipt, first_request(&sources)?)?;
        let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
            anyhow::bail!("healthy checked operation must retain actual U");
        };
        assert_eq!(source.event_id, sources[1].obj_id());
        assert!(h.network.repo.find_one(sources[0].obj_id()).await?.is_some());
        assert!(h.network.repo.find_one(sources[1].obj_id()).await?.is_some());
        assert_eq!(canonical.0.value.members().len(), 1);
        assert_eq!(local.0.value.members().len(), 1);
        let log = remote.find_tail_event(VaultLogDescriptor::from(h.actor.vault_name.clone()))
            .await?.context("actual no-fault canonical result")?;
        let request_id = receipt.request_id.as_ref().or_else(||
            current.as_ref().and_then(|context| context.request_id.as_ref()));
        if let Some(id) = request_id {
            assert!(log.0.value.liveboard_removal_results.iter().any(|result|
                result.request_id == *id && serde_json::to_value(result).is_ok_and(|wire|
                    wire["applicationState"] == "applied")));
        }
        assert_eq!(receipt.material_stage, Some(ProducerMaterialStage::Ready));
        assert_eq!(receipt.canonical_state, ProducerCanonicalState::Applied,
            "RED: healthy checked2→1 lost actual canonical operation result");
        assert_eq!(receipt.phase, ProducerPhase::Queued,
            "RED: healthy checked2→1 cannot remain Unknown after actual R/U Applied/materialReady");
        assert!(["ready", "done", "settled"].contains(&receipt.readiness.as_str()),
            "RED: healthy checked2→1 never reaches its final context-save/clear predicate");
        assert!(current.is_none(), "healthy checked operation must clear its actual context");
        Ok(())
    }).await
}
