//! Receiver response authorization must not block requester consumption/completion.
use super::*;
use meta_secret_core::crypto::key_pair::KeyPair;
use meta_secret_core::node::api::{SignedActionPayload, SsRecoveryCompletion};
use meta_secret_core::node::common::model::{
    crypto::aead::EncryptedMessage,
    secret::{SecretDistributionData, SsClaim, SsDeclineData, SsDistributionStatus},
};
use meta_secret_core::node::db::{
    descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
    events::shared_secret_event::SsWorkflowObject,
    objects::persistent_shared_secret::PersistentSharedSecret,
};
use meta_secret_core::node::security::sign_recovery_completion;
use meta_secret_core::secret::shared_secret::PlainText;

fn workflow(
    h: &Harness,
    actor: &UserCreds,
    claim: &SsClaim,
    accepted: bool,
) -> Result<SsWorkflowObject> {
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == actor.device_id())
        .context("canonical receiver")?;
    Ok(if accepted {
        // Server controls validate the signed encrypted packet/channel, not its decrypted content.
        let share = actor.device_creds.key_manager()?.transport.encrypt_string(
            PlainText::from("authorization-control-not-a-crypto-fixture"),
            &h.a.device_creds.key_manager()?.transport.pk(),
        )?;
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
    })
}
fn signed(actor: &UserCreds, workflow: SsWorkflowObject) -> Result<SignedAction> {
    sign_event(
        workflow.to_generic(),
        actor.device_id().clone(),
        &actor.device_creds.key_manager()?.dsa,
    )
}
async fn log(h: &Harness) -> Result<meta_secret_core::node::common::model::secret::SsLogData> {
    PersistentSharedSecret::from(h.p_obj.clone())
        .get_ss_log_obj(h.a.vault_name.clone())
        .await
}

#[tokio::test]
async fn liveboard_authorization_both_addressed_receivers_can_send_a_real_signed_approve_or_decline()
-> Result<()> {
    for receiver_b in [true, false] {
        for accepted in [true, false] {
            let h = Harness::new().await?;
            let claim = super::recovery::pending_claim(&h).await?;
            let actor = if receiver_b { &h.b } else { &h.c };
            send(
                &h.server,
                signed(actor, workflow(&h, actor, &claim, accepted)?)?,
            )
            .await?;
            let state = log(&h).await?;
            let winner = &state.liveboard_recovery_results[&claim.id];
            assert_eq!(winner.responder_id, *actor.device_id());
            assert_eq!(
                state.claims[&claim.id].status.get(actor.device_id()),
                Some(&if accepted {
                    SsDistributionStatus::Sent
                } else {
                    SsDistributionStatus::Declined
                })
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn liveboard_authorization_wrong_receiver_claim_vault_stream_or_channel_is_rejected_before_write()
-> Result<()> {
    for invalid in ["actor", "claim", "vault", "stream", "channel"] {
        let h = Harness::new().await?;
        let claim = super::recovery::pending_claim(&h).await?;
        let mut packet = workflow(&h, &h.b, &claim, true)?;
        let SsWorkflowObject::Recovery(ref mut data) = packet else {
            unreachable!()
        };
        match invalid {
            "claim" => {
                data.value.claim_id.pass_id =
                    meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str(
                        "another-pass",
                    )
            }
            "vault" => data.value.vault_name = "another-vault".into(),
            "stream" => {
                let wrong = claim
                    .recovery_db_ids()
                    .into_iter()
                    .find(|id| id.distribution_id.receiver == *h.c.device_id())
                    .context("other receiver")?;
                data.key = KvKey::from(SsWorkflowDescriptor::Recovery(wrong));
            }
            "channel" => {
                let share = h.b.device_creds.key_manager()?.transport.encrypt_string(
                    PlainText::from("bad-target-control"),
                    &h.c.device_creds.key_manager()?.transport.pk(),
                )?;
                data.value.secret_message = EncryptedMessage::CipherShare { share };
            }
            _ => {}
        }
        let signer = if invalid == "actor" { &h.c } else { &h.b };
        let action = signed(signer, packet)?;
        action.verify(&signer.device().keys.dsa_pk)?;
        assert!(matches!(action.payload, SignedActionPayload::Event(_)));
        let before = h.p_obj.repo.get_db().await;
        assert!(
            send(&h.server, action).await.is_err(),
            "invalid {invalid} response must reject"
        );
        assert!(
            h.p_obj.repo.get_db().await == before,
            "invalid {invalid} response changed persistent state"
        );
    }
    Ok(())
}

async fn completion(
    h: &Harness,
    claim: &SsClaim,
    status: SsDistributionStatus,
) -> Result<DataSyncResponse> {
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == *h.b.device_id())
        .context("approved receiver")?;
    let action = sign_recovery_completion(
        SsRecoveryCompletion {
            vault_name: claim.vault_name.clone(),
            recovery_id: id,
            receiver_status: status,
        },
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    h.server
        .handle_client_request(SyncRequest::Read(Box::new(
            ReadSyncRequest::SsRecoveryCompletion(action),
        )))
        .await
}

#[tokio::test]
async fn liveboard_authorization_requester_sent_completion_after_addressed_receiver_approval_still_succeeds()
-> Result<()> {
    let h = Harness::new().await?;
    let claim = super::recovery::pending_claim(&h).await?;
    send(&h.server, signed(&h.b, workflow(&h, &h.b, &claim, true)?)?).await?;
    assert!(matches!(
        completion(&h, &claim, SsDistributionStatus::Sent).await?,
        DataSyncResponse::Data(_)
    ));
    let state = log(&h).await?;
    assert_eq!(
        state.claims[&claim.id].status.get(h.a.device_id()),
        Some(&SsDistributionStatus::Delivered)
    );
    assert_eq!(
        state.claims[&claim.id].status.get(h.b.device_id()),
        Some(&SsDistributionStatus::Sent)
    );
    assert_eq!(
        state.liveboard_recovery_results[&claim.id].responder_id,
        *h.b.device_id()
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_authorization_requester_cannot_complete_sent_before_approval_or_decline_after_approval()
-> Result<()> {
    let h = Harness::new().await?;
    let claim = super::recovery::pending_claim(&h).await?;
    let before = h.p_obj.repo.get_db().await;
    assert!(
        completion(&h, &claim, SsDistributionStatus::Sent)
            .await
            .is_err()
    );
    assert!(h.p_obj.repo.get_db().await == before);
    send(&h.server, signed(&h.b, workflow(&h, &h.b, &claim, true)?)?).await?;
    let approved = h.p_obj.repo.get_db().await;
    assert!(
        completion(&h, &claim, SsDistributionStatus::Declined)
            .await
            .is_err()
    );
    assert!(h.p_obj.repo.get_db().await == approved);
    Ok(())
}
