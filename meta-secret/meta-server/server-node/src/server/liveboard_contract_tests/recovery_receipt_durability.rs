use super::recovery_receipt_faults::{Fault, ReceiptFaultRepo, Step};
use super::recovery_receipt_fixture::ReceiptHarness;
use super::*;
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::sync::atomic::Ordering;

async fn fault_case(accepted: bool, step: Step, fault: Fault) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let packet = h.packet(&h.base.b, accepted)?;
    let server = h.server().await?;
    repo.arm(step, fault);
    assert!(
        send(&server, packet.clone()).await.is_err(),
        "RED: uncertain persistence must not acknowledge success"
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "RED: real required persistence phase was not reached"
    );
    drop(server);
    repo.clear();
    let reopened = h.server().await?;
    refresh(&reopened, &h.base.a).await?;
    if step != Step::Reserve || fault != Fault::Before {
        // Durable reservation must finish before retransmission or an opposite receiver.
        let first = h.assert_winner(accepted).await?;
        let _ = send(&reopened, h.packet(&h.base.c, !accepted)?).await;
        assert!(
            h.assert_winner(accepted).await? == first,
            "late opposite response replaced durable reservation"
        );
    }
    send(&reopened, packet)
        .await
        .context("RED: exact verified source retry must acknowledge without a second transition")?;
    h.assert_winner(accepted).await?;
    assert!(
        inner
            .server_recovery_response_store()?
            .load(&h.claim.vault_name)
            .await?
            .is_none(),
        "private cleanup must be verified before success"
    );
    Ok(())
}

macro_rules! case {
    ($name:ident, $accepted:expr, $step:ident, $fault:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            fault_case($accepted, Step::$step, Fault::$fault).await
        }
    };
}
case!(
    recovery_receipt_approve_reserve_before,
    true,
    Reserve,
    Before
);
case!(
    recovery_receipt_decline_reserve_before,
    false,
    Reserve,
    Before
);
case!(
    recovery_receipt_approve_reserve_lost_reply,
    true,
    Reserve,
    LostReply
);
case!(
    recovery_receipt_decline_reserve_lost_reply,
    false,
    Reserve,
    LostReply
);
case!(
    recovery_receipt_approve_reserve_readback,
    true,
    Reserve,
    Readback
);
case!(
    recovery_receipt_decline_reserve_readback,
    false,
    Reserve,
    Readback
);
case!(recovery_receipt_approve_mark_before, true, Mark, Before);
case!(recovery_receipt_decline_mark_before, false, Mark, Before);
case!(
    recovery_receipt_approve_mark_lost_reply,
    true,
    Mark,
    LostReply
);
case!(
    recovery_receipt_decline_mark_lost_reply,
    false,
    Mark,
    LostReply
);
case!(recovery_receipt_approve_mark_readback, true, Mark, Readback);
case!(
    recovery_receipt_decline_mark_readback,
    false,
    Mark,
    Readback
);
case!(recovery_receipt_approve_delete_before, true, Remove, Before);
case!(
    recovery_receipt_decline_delete_before,
    false,
    Remove,
    Before
);
case!(
    recovery_receipt_approve_delete_lost_reply,
    true,
    Remove,
    LostReply
);
case!(
    recovery_receipt_decline_delete_lost_reply,
    false,
    Remove,
    LostReply
);
case!(
    recovery_receipt_approve_delete_readback,
    true,
    Remove,
    Readback
);
case!(
    recovery_receipt_decline_delete_readback,
    false,
    Remove,
    Readback
);
case!(recovery_receipt_approve_raw_before, true, Raw, Before);
case!(recovery_receipt_decline_raw_before, false, Raw, Before);
case!(
    recovery_receipt_approve_raw_lost_reply,
    true,
    Raw,
    LostReply
);
case!(
    recovery_receipt_decline_raw_lost_reply,
    false,
    Raw,
    LostReply
);
case!(recovery_receipt_approve_raw_readback, true, Raw, Readback);
case!(recovery_receipt_decline_raw_readback, false, Raw, Readback);
case!(
    recovery_receipt_approve_canonical_before,
    true,
    Canonical,
    Before
);
case!(
    recovery_receipt_decline_canonical_before,
    false,
    Canonical,
    Before
);
case!(
    recovery_receipt_approve_canonical_lost_reply,
    true,
    Canonical,
    LostReply
);
case!(
    recovery_receipt_decline_canonical_lost_reply,
    false,
    Canonical,
    LostReply
);
case!(
    recovery_receipt_approve_canonical_readback,
    true,
    Canonical,
    Readback
);
case!(
    recovery_receipt_decline_canonical_readback,
    false,
    Canonical,
    Readback
);

#[tokio::test]
async fn recovery_receipt_prepared_with_canonical_proof_does_not_resurrect_consumed_packet()
-> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner.clone()));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let original = h.packet(&h.base.b, true)?;
    let source_id = original.event()?.obj_id();
    let server = h.server().await?;
    repo.arm(Step::Mark, Fault::Before);
    assert!(
        send(&server, original.clone()).await.is_err(),
        "RED: private phase failure must remain unavailable"
    );
    h.assert_winner(true).await?;
    let prepared = inner
        .server_recovery_response_store()?
        .load(&h.claim.vault_name)
        .await?
        .context("durable Prepared row")?;
    assert!(prepared.phase == meta_secret_core::node::db::repo::recovery_response_store::RecoveryResponsePhase::Prepared);
    // The existing recipient-specific delivery consumes the actual raw workflow.
    let gateway = super::super::server_data_sync::ServerSyncGateway::new(
        Arc::new(PersistentObject::new(inner.clone())),
        Arc::new(super::super::state_invalidation::NoopStateInvalidationPublisher),
    );
    let response = gateway
        .ss_replication(
            meta_secret_core::node::api::SsRequest {
                sender: h.base.a.user(),
                ss_log: prepared.base_ss_log_artifact_id.clone(),
            },
            h.base.c.device_id().clone(),
        )
        .await?;
    assert!(
        response.iter().any(|event| event.obj_id() == source_id),
        "actual requester delivery must include the approved encrypted packet"
    );
    assert!(inner.find_one(source_id.clone()).await?.is_none());
    repo.clear();
    drop(server);
    let reopened = h.server().await?;
    refresh(&reopened, &h.base.a).await?;
    send(&reopened, original.clone()).await?;
    assert!(
        inner.find_one(source_id.clone()).await?.is_none(),
        "canonical same-proof repair/ack must never recreate consumed ciphertext"
    );
    let mut changed = original.clone();
    changed.nonce += 1;
    assert!(
        send(&reopened, changed).await.is_err(),
        "a changed source cannot use the exact-source replay exception"
    );
    assert!(matches!(
        reopened.handle_client_request(h.completion()?).await?,
        DataSyncResponse::Data(_)
    ));
    assert!(
        h.log().await?.claims[&h.claim.id]
            .status
            .get(h.base.a.device_id())
            == Some(
                &meta_secret_core::node::common::model::secret::SsDistributionStatus::Delivered
            ),
        "requester Sent completion must retain its separate valid role"
    );
    send(&reopened, original).await?;
    assert!(
        inner.find_one(source_id).await?.is_none(),
        "exact retry after requester completion must remain a no-mutation acknowledgement"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_exact_retry_is_idempotent_but_changed_signed_body_is_not() -> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    let original = h.packet(&h.base.b, true)?;
    send(&server, original.clone()).await?;
    let settled = repo.get_db().await;
    send(&server, original)
        .await
        .context("RED: compact verified source proof must acknowledge exact retry")?;
    assert!(
        repo.get_db().await == settled,
        "exact acknowledgement is not a second application"
    );
    let different = h.packet(&h.base.b, true)?; // Fresh real randomized ciphertext, same actor/claim/nonce.
    assert!(
        send(&server, different).await.is_err(),
        "same source ID with a different valid signed body is not exact replay"
    );
    assert!(repo.get_db().await == settled);
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_control_valid_requester_sent_completion_after_receiver_approval()
-> Result<()> {
    let h = ReceiptHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    send(&server, h.packet(&h.base.b, true)?).await?;
    assert!(matches!(
        server.handle_client_request(h.completion()?).await?,
        DataSyncResponse::Data(_)
    ));
    assert!(
        h.log().await?.claims[&h.claim.id]
            .status
            .get(h.base.a.device_id())
            == Some(
                &meta_secret_core::node::common::model::secret::SsDistributionStatus::Delivered
            )
    );
    Ok(())
}

async fn opposite_without_refresh(accepted: bool) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(ReceiptFaultRepo::new(inner));
    let h = ReceiptHarness::new(repo.clone()).await?;
    let server = h.server().await?;
    repo.arm(Step::Reserve, Fault::LostReply);
    assert!(
        send(&server, h.packet(&h.base.b, accepted)?).await.is_err(),
        "RED: uncertain reserve is not success"
    );
    repo.clear();
    drop(server);
    let reopened = h.server().await?;
    let _ = send(&reopened, h.packet(&h.base.c, !accepted)?).await;
    h.assert_winner(accepted).await?;
    Ok(())
}

#[tokio::test]
async fn recovery_receipt_approve_reservation_repairs_before_opposite_receiver_without_refresh()
-> Result<()> {
    opposite_without_refresh(true).await
}

#[tokio::test]
async fn recovery_receipt_decline_reservation_repairs_before_opposite_receiver_without_refresh()
-> Result<()> {
    opposite_without_refresh(false).await
}
