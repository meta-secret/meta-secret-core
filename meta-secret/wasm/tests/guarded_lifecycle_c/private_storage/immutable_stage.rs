//! Corrupt only a REAL full producer stage left by an origin-before-body fault.
use super::{
    repo::{Fault, Point},
    stage::private_crypto,
    support::{H, ordinary},
    *,
};
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        common::model::crypto::aead::EncryptedMessage,
        db::events::shared_secret_event::SsWorkflowObject, security::canonical_json,
    },
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
enum Corruption {
    MissingRequestedPass,
    CompactClaimMismatch,
    ValidCipherMismatch,
}

fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(value)?)?))
}

#[tokio::test]
async fn full_actual_stage_rejects_missing_requested_pass_compact_claim_or_frozen_cipher_change()
-> Result<()> {
    let mut results = vec![];
    for variant in [
        Corruption::MissingRequestedPass,
        Corruption::CompactClaimMismatch,
        Corruption::ValidCipherMismatch,
    ] {
        results.push(run(variant).await);
    }
    // All three actual producer/retry paths execute before propagating first failure.
    for result in results {
        result?;
    }
    Ok(())
}

async fn run(variant: Corruption) -> Result<()> {
    let h = H::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let original_live = ordinary(&h.p.repo.snapshot().await?);
    h.p.repo.arm(Point::Origin, Fault::Before);
    let interrupted = h.decide().await;
    ensure!(
        interrupted
            .err()
            .is_some_and(|e| e.to_string() == "GuardedPreparationUnavailable")
            && h.p.repo.hits.load(Ordering::SeqCst) > 0,
        "real origin-before-body interruption must be reached"
    );
    h.no_body_or_send().await?;
    let frozen = h.witness().await?;
    ensure!(
        frozen.0.value.phase == GuardedPreparationPhase::Staged && frozen.0.value.origin.is_none(),
        "actual FULL Staged witness without a queued body required"
    );
    private_crypto(&h.state, &frozen)?;
    let payload = frozen.0.value.payload.as_ref().unwrap();
    let expected_passes: HashSet<_> = payload
        .update
        .request
        .share_transfers
        .iter()
        .filter(|transfer| transfer.receiver == *h.state.b.device_id())
        .map(|transfer| transfer.pass_id.clone())
        .collect();
    ensure!(
        expected_passes.len() == 2
            && payload
                .passes
                .iter()
                .map(|pass| pass.pass_id.clone())
                .collect::<HashSet<_>>()
                == expected_passes,
        "healthy full stage must cover both ACTUAL B-addressed request Passes"
    );
    ensure!(
        frozen.0.value.claims.len() == 2 && frozen.0.value.packets.len() == 4,
        "healthy compact ownership must cover real frozen stage"
    );
    for pass in &payload.passes {
        ensure!(
            frozen
                .0
                .value
                .claims
                .iter()
                .any(|claim| claim.pass_id == pass.pass_id && claim.claim == pass.claim),
            "actual producer compact claim differs before corruption"
        );
        for packet in &pass.distributions {
            let compact = frozen
                .0
                .value
                .packets
                .iter()
                .find(|compact| compact.event_id == packet.obj_id())
                .context("actual compact packet ownership")?;
            ensure!(
                compact.body_commitment == digest(packet)?,
                "actual producer compact digest differs before corruption"
            );
        }
    }
    let original_update = payload.update.clone();
    let mut corrupt = frozen.clone();
    match variant {
        Corruption::MissingRequestedPass => {
            // Keep metadata internally count-consistent: ONLY the actual request exposes omission.
            let removed = corrupt
                .0
                .value
                .payload
                .as_mut()
                .unwrap()
                .passes
                .pop()
                .unwrap();
            let removed_ids: HashSet<_> = removed
                .distributions
                .iter()
                .map(ObjIdExtractor::obj_id)
                .collect();
            corrupt
                .0
                .value
                .claims
                .retain(|claim| claim.pass_id != removed.pass_id);
            corrupt
                .0
                .value
                .packets
                .retain(|packet| !removed_ids.contains(&packet.event_id));
            ensure!(
                corrupt.0.value.payload.as_ref().unwrap().passes.len() == 1
                    && corrupt.0.value.claims.len() == 1
                    && corrupt.0.value.packets.len() == 2,
                "omitted Pass fixture must keep compact counts consistent"
            );
        }
        Corruption::CompactClaimMismatch => {
            corrupt
                .0
                .value
                .claims
                .pop()
                .context("actual compact claim to remove")?;
            ensure!(
                corrupt.0.value.payload == frozen.0.value.payload,
                "compact-only corruption must leave FULL frozen payload unchanged"
            );
        }
        Corruption::ValidCipherMismatch => {
            let packet = &mut corrupt.0.value.payload.as_mut().unwrap().passes[0].distributions[0];
            let SsWorkflowObject::Distribution(event) = packet else {
                anyhow::bail!("actual frozen Distribution required");
            };
            let original_key = event.key.clone();
            let original_claim = event.value.claim_id.clone();
            let old = event.value.secret_message.cipher_text().clone();
            let recipient = old.channel.receiver().to_device_id();
            let user = if recipient == *h.state.a.device_id() {
                &h.state.a
            } else {
                ensure!(
                    recipient == *h.state.b.device_id(),
                    "actual survivor recipient required"
                );
                &h.state.b
            };
            let plain = old.decrypt(&user.device_creds.secret_box.transport.sk)?;
            let encrypted = plain.encrypt()?; // Existing real crypto, SAME plaintext and channel.
            ensure!(
                encrypted.channel == old.channel
                    && encrypted.msg != old.msg
                    && encrypted
                        .decrypt(&user.device_creds.secret_box.transport.sk)?
                        .msg
                        == plain.msg,
                "cipher mutation must remain valid same-share encryption"
            );
            event.value.secret_message = EncryptedMessage::CipherShare { share: encrypted };
            ensure!(
                event.key == original_key && event.value.claim_id == original_claim,
                "valid cipher mutation must preserve ID and claim generation"
            );
            let packet_id = packet.obj_id();
            let compact = corrupt
                .0
                .value
                .packets
                .iter()
                .find(|compact| compact.event_id == packet_id)
                .unwrap();
            ensure!(
                compact.body_commitment != digest(packet)?
                    && corrupt.0.value.packets == frozen.0.value.packets,
                "compact original commitment MUST remain unchanged and mismatch altered cipher"
            );
        }
    }
    ensure!(
        corrupt.0.value.payload.as_ref().unwrap().update == original_update,
        "actual signed request/recipient Pass set must remain intact"
    );
    h.p.repo.inner.replace(corrupt.clone()).await?; // Labelled local data corruption only.
    h.p.repo.clear();
    let before_retry = h.p.repo.snapshot().await?;
    let result = h.decide().await;
    let after_retry = h.p.repo.snapshot().await?;
    let private_unchanged = h.witness().await? == corrupt;
    let no_body = h.no_body_or_send().await.is_ok();
    eprintln!(
        "C1b full-stage {variant:?} unavailable={} db_unchanged={} corrupt_private_retained={private_unchanged} no_body_or_send={no_body} live_unchanged={}",
        result.is_err(),
        after_retry == before_retry,
        ordinary(&after_retry) == original_live
    );
    let error = result
        .err()
        .context("explicit retry accepted corrupted full frozen preparation")?;
    ensure!(
        error.to_string() == "GuardedPreparationUnavailable",
        "corrupt full stage must use existing static unavailable"
    );
    ensure!(
        after_retry == before_retry
            && private_unchanged
            && no_body
            && ordinary(&after_retry) == original_live,
        "corrupt stage was healed/recomputed/requeued or published"
    );
    Ok(())
}
