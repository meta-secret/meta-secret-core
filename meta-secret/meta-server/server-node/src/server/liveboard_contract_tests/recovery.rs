use super::*;
use meta_secret_core::crypto::key_pair::KeyPair;
use meta_secret_core::crypto::utils::Id48bit;
use meta_secret_core::node::common::model::crypto::aead::EncryptedMessage;
use meta_secret_core::node::common::model::meta_pass::MetaPasswordId;
use meta_secret_core::node::common::model::secret::{
    ClaimId, SecretDistributionData, SecretDistributionType, SsClaim, SsClaimId, SsDeclineData,
    SsDistributionCompositeStatus, SsDistributionStatus, SsLogData,
};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::{
    SsDeviceLogDescriptor, SsWorkflowDescriptor,
};
use meta_secret_core::node::db::events::shared_secret_event::{
    SsDeviceLogObject, SsWorkflowObject,
};
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::secret::shared_secret::PlainText;

pub(super) async fn pending_claim(h: &Harness) -> Result<SsClaim> {
    let pass = MetaPasswordId::build_from_str("liveboard-responder");
    let p_vault = PersistentVault::from(h.p_obj.clone());
    let mut vault = p_vault.get_vault(h.a.vault_name.clone()).await?;
    vault.0.value = vault.0.value.add_secret(pass.clone());
    h.p_obj.repo.replace(vault).await?;
    let id = ClaimId(Id48bit::generate());
    let receivers = vec![h.b.device_id().clone(), h.c.device_id().clone()];
    let claim = SsClaim {
        id: id.clone(),
        dist_claim_id: SsClaimId { id, pass_id: pass },
        vault_name: h.a.vault_name.clone(),
        sender: h.a.device_id().clone(),
        distribution_type: SecretDistributionType::Recover,
        status: SsDistributionCompositeStatus::from(receivers.clone()),
        receivers,
        client_status: None,
    };
    PersistentSharedSecret::from(h.p_obj.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    Ok(claim)
}

async fn response(h: &Harness, actor: &UserCreds, claim: &SsClaim, accepted: bool) -> Result<()> {
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == actor.device_id())
        .context("test responder is not in receiver list")?;
    let workflow = if accepted {
        let receiver = h.a.device_creds.key_manager()?.transport.pk();
        let share = actor
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(PlainText::from("synthetic-test-share"), &receiver)?;
        SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(id)),
            value: SecretDistributionData {
                vault_name: claim.vault_name.clone(),
                claim_id: claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share },
            },
        })
    } else {
        SsWorkflowObject::Decline(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Decline(id)),
            value: SsDeclineData {
                vault_name: claim.vault_name.clone(),
                claim_id: claim.id.clone(),
                receiver_id: actor.device_id().clone(),
            },
        })
    };
    let signed = sign_event(
        workflow.to_generic(),
        actor.device_id().clone(),
        &actor.device_creds.key_manager()?.dsa,
    )?;
    send(&h.server, signed).await
}

async fn recovery_log(h: &Harness) -> Result<SsLogData> {
    PersistentSharedSecret::from(h.p_obj.clone())
        .get_ss_log_obj(h.a.vault_name.clone())
        .await
}

fn winner(log: &SsLogData, claim: &SsClaim) -> Result<Value> {
    let wire = serde_json::to_value(log)?;
    let key = serde_json::to_value(&claim.id)?;
    let key = key.as_str().context("ClaimId wire key")?;
    wire.get("liveboardRecoveryResults")
        .and_then(|results| results.get(key))
        .cloned()
        .context(
            "RED: verified first recovery responder is not durably retained in canonical SsLog",
        )
}

#[tokio::test]
async fn liveboard_first_verified_decline_retains_actual_responder_when_all_receivers_are_retired()
-> Result<()> {
    let h = Harness::new().await?;
    let claim = pending_claim(&h).await?;
    response(&h, &h.b, &claim, false).await?;
    let log = recovery_log(&h).await?;
    assert!(
        claim
            .receivers
            .iter()
            .all(|receiver| log.claims[&claim.id].status.get(receiver)
                == Some(&SsDistributionStatus::Declined)),
        "first decline still retires every receiver"
    );
    let first = winner(&log, &claim)?;
    assert_eq!(first["responderId"], serde_json::to_value(h.b.device_id())?);
    assert_eq!(first["claimId"], serde_json::to_value(&claim.id)?);
    assert_eq!(first["outcome"], "declined");
    response(&h, &h.c, &claim, false).await?;
    response(&h, &h.c, &claim, true).await?;
    refresh(&h.reopen().await?, &h.a).await?;
    assert_eq!(
        winner(&recovery_log(&h).await?, &claim)?,
        first,
        "late responses must not rewrite verified responder"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_first_verified_approval_keeps_winner_through_late_decline_and_other_approval()
-> Result<()> {
    let h = Harness::new().await?;
    let claim = pending_claim(&h).await?;
    response(&h, &h.b, &claim, true).await?;
    let log = recovery_log(&h).await?;
    assert_eq!(
        log.claims[&claim.id].status.get(h.b.device_id()),
        Some(&SsDistributionStatus::Sent)
    );
    let first = winner(&log, &claim)?;
    assert_eq!(first["responderId"], serde_json::to_value(h.b.device_id())?);
    assert_eq!(first["outcome"], "accepted");
    response(&h, &h.c, &claim, false).await?;
    response(&h, &h.c, &claim, true).await?;
    assert_eq!(winner(&recovery_log(&h).await?, &claim)?, first);
    assert!(
        !serde_json::to_string(&first)?.contains("secretMessage"),
        "decision metadata must exclude recovery ciphertext"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_membership_retirement_and_historical_decline_never_invent_a_responder()
-> Result<()> {
    let h = Harness::new().await?;
    let claim = pending_claim(&h).await?;
    let p_ss = PersistentSharedSecret::from(h.p_obj.clone());
    let retired = recovery_log(&h).await?.decline_pending_recovery_claims();
    let object = p_ss
        .create_new_ss_log_object(retired, h.a.vault_name.clone())
        .await?;
    h.p_obj.repo.save(object).await?;
    h.refresh().await?;
    let wire = serde_json::to_value(recovery_log(&h).await?)?;
    let results = wire.get("liveboardRecoveryResults");
    assert!(
        results.is_none_or(|value| value.as_object().is_some_and(|map| map.is_empty())),
        "a historical/membership decline is not proof that any named device replied"
    );
    assert!(
        recovery_log(&h).await?.claims[&claim.id]
            .receivers
            .iter()
            .all(|receiver| recovery_log_status(&wire, &claim, receiver) == Some("declined"))
    );
    Ok(())
}

fn recovery_log_status<'a>(
    wire: &'a Value,
    claim: &SsClaim,
    receiver: &meta_secret_core::node::common::model::device::common::DeviceId,
) -> Option<&'a str> {
    let claim_id = serde_json::to_value(&claim.id).ok()?;
    let receiver_id = serde_json::to_value(receiver).ok()?;
    wire["claims"][claim_id.as_str()?]["status"]["statuses"][receiver_id.as_str()?].as_str()
}

#[tokio::test]
async fn liveboard_signed_upload_cannot_forge_canonical_recovery_winner() -> Result<()> {
    let h = Harness::new().await?;
    let claim = pending_claim(&h).await?;
    let p_ss = PersistentSharedSecret::from(h.p_obj.clone());
    let object = p_ss
        .find_ss_log_tail_event(h.a.vault_name.clone())
        .await?
        .context("canonical claim log")?;
    let signed = sign_event(
        object.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    let before = h.p_obj.repo.get_db().await;
    assert!(
        send(&h.server, signed).await.is_err(),
        "client-signed canonical SsLog is never an allowed network write"
    );
    assert!(h.p_obj.repo.get_db().await == before);

    let object = SsDeviceLogObject(KvLogEvent {
        key: KvKey::from(SsDeviceLogDescriptor::from(h.a.device_id().clone())),
        value: claim.clone(),
    });
    let mut wire = serde_json::to_value(sign_event(
        object.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?)?;
    wire["payload"]["event"]["ssDeviceLog"]["value"]["liveboardRecoveryResults"] =
        json!({"forged":{"responderId":h.c.device_id(),"outcome":"accepted"}});
    let decoded: SignedAction = serde_json::from_value(wire)?;
    // Unknown data must either be excluded by the typed signed-claim shape or
    // fail verification; it must never become a canonical winner.
    let _ = send(&h.server, decoded).await;
    let canonical = serde_json::to_value(recovery_log(&h).await?)?;
    assert!(
        canonical
            .get("liveboardRecoveryResults")
            .is_none_or(|value| value.as_object().is_some_and(|map| map.is_empty())),
        "untrusted claim fields cannot supply a verified responder"
    );
    Ok(())
}
