//! Native invalid-data controls preserve strict current3/recovery source validation.
use super::fixture::*;
use crate::{
    node::{
        common::model::{crypto::aead::EncryptedMessage, secret::*, user::user_creds::UserCreds},
        db::{
            descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
            events::{kv_log_event::*, shared_secret_event::SsWorkflowObject},
            repo::generic_db::*,
        },
    },
    secret::shared_secret::{PlainText, UserShareDto},
};
use anyhow::{Context, Result};
async fn response(
    h: &Fixture,
    actor: &UserCreds,
    claim: &SsClaim,
    edit: impl FnOnce(&mut UserShareDto),
) -> Result<()> {
    let peer =
        h.p.find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: h.pass.clone(),
            receiver: h.peer.device_id().clone(),
        }))
        .await?
        .context("real strict peer share")?
        .to_distribution_data()?;
    let clear = peer
        .secret_message
        .cipher_text()
        .decrypt(&h.peer.device_creds.secret_box.transport.sk)?;
    let mut share = UserShareDto::try_from(&clear.msg)?;
    edit(&mut share);
    let encrypted = actor.device_creds.key_manager()?.transport.encrypt_string(
        PlainText::from(share.as_json()?),
        &h.creds.device().keys.transport_pk,
    )?;
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == *h.peer.device_id())
        .context("actual expected recovery descriptor")?;
    h.p.repo
        .save(SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(id)),
            value: SecretDistributionData {
                vault_name: h.creds.vault_name.clone(),
                claim_id: claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share: encrypted },
            },
        }))
        .await?;
    Ok(())
}
async fn run(wrong_source: bool, k1: bool) -> Result<bool> {
    let h = Fixture::new(3, 3).await?;
    let local = h
        .material()
        .local(&h.pass)
        .await?
        .context("strict local k2 share")?;
    let mut claim = h.member.create_recovery_claim(h.pass.clone());
    claim
        .status
        .statuses
        .insert(h.peer.device_id().clone(), SsDistributionStatus::Sent);
    response(
        &h,
        if wrong_source { &h.creds } else { &h.peer },
        &claim,
        |s| {
            if k1 {
                for b in &mut s.share_blocks {
                    b.config.threshold = 1;
                }
            }
        },
    )
    .await?;
    let before = h.p.repo.get_db().await;
    let ready = h
        .material()
        .recovered(&claim, Some(&local))
        .await
        .unwrap_or(false);
    assert!(before == h.p.repo.get_db().await);
    Ok(ready)
}
#[tokio::test]
async fn exact_current3_approved_receiver_k2_material_control() -> Result<()> {
    assert!(run(false, false).await?);
    Ok(())
}
#[tokio::test]
async fn different_response_sender_cannot_use_whole_copy_origin_exception() -> Result<()> {
    assert!(!run(true, false).await?);
    Ok(())
}
#[tokio::test]
async fn recovery_response_k1_cannot_use_whole_copy_profile() -> Result<()> {
    assert!(!run(false, true).await?);
    Ok(())
}
#[tokio::test]
async fn current3_split_source_stays_bound_to_canonical_sender() -> Result<()> {
    let mut h = Fixture::new(3, 3).await?;
    h.claims.claims.values_mut().next().unwrap().sender = h.peer.device_id().clone();
    h.denied().await
}
