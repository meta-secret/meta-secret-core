use super::{
    recovery_inflight_fixture::Inflight,
    recovery_intake_faults::{Fault, IntakeFaultRepo, Phase},
    recovery_intake_fixture::assert_clean,
    *,
};
use meta_secret_core::{
    crypto::utils::sha256_hex, node::db::events::shared_secret_event::SsDeviceLogObject,
};
use std::sync::atomic::Ordering;

pub(super) struct Continuation {
    pub a: UserCreds,
    pub master: TransportSk,
    pub source: SsDeviceLogObject,
    pub signed: SignedAction,
    pub target: ArtifactId,
    pub admission: VaultObject,
}
pub(super) async fn interrupt<R: KvLogEventRepo>(
    inner: Arc<R>,
    phase: Phase,
    fault: Fault,
) -> Result<Continuation> {
    let repo = Arc::new(IntakeFaultRepo::new(inner.clone()));
    let h = Inflight::new(repo.clone()).await?;
    let target = h.ss_base.obj_id().next();
    repo.arm(phase, fault);
    let result = send(&h.server, h.signed.clone()).await;
    eprintln!(
        "[INFLIGHT_PHASE] {phase:?}/{fault:?} hits={} reads={} calls={:?}",
        repo.hits.load(Ordering::SeqCst),
        repo.read_hits.load(Ordering::SeqCst),
        repo.calls.lock().unwrap()
    );
    assert!(
        result.is_err(),
        "RED: old-topology intake must propagate interrupted persistence"
    );
    assert!(
        repo.hits.load(Ordering::SeqCst) > 0,
        "RED: required old-topology intake fault phase not reached"
    );
    if phase == Phase::Canonical {
        assert!(
            inner.find_one(h.source.obj_id()).await?.is_none(),
            "RED: original raw advanced before canonical proof"
        );
    }
    let log = h.log().await?;
    if !matches!(
        (phase, fault),
        (Phase::Canonical, Fault::Before | Fault::Drop) | (Phase::Cleanup, Fault::LostReply)
    ) {
        let receipt = log
            .0
            .value
            .pending_recovery_request_intake
            .as_ref()
            .context("RED: interrupted old-topology source lacks its trusted Prepared receipt")?;
        assert_eq!(
            receipt.verified_vault_artifact_id,
            h.admission.obj_id(),
            "capture actual post-removal context, never original topology"
        );
        assert_eq!(receipt.base_ss_log_artifact_id, Some(h.ss_base.obj_id()));
        assert_eq!(receipt.target_ss_log_artifact_id, target);
        assert!(receipt.source_key == h.source.0.key);
        assert_eq!(receipt.claim_id, h.source.0.value.id);
        assert_eq!(
            receipt.verified_action_hash,
            sha256_hex(&String::from_utf8(canonical_json(&h.signed)?)?)
        );
        assert!(log.0.value.claims.get(&h.source.0.value.id) == Some(&h.source.0.value));
    }
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    let current = PersistentVault::from(Arc::new(PersistentObject::new(inner)))
        .get_vault(h.base.a.vault_name.clone())
        .await?;
    assert!(
        current == h.admission,
        "intake must not change the completed removal"
    );
    Ok(Continuation {
        a: h.base.a,
        master: h.base.master_key,
        source: h.source,
        signed: h.signed,
        target,
        admission: h.admission,
    })
}
pub(super) async fn reopen<R: KvLogEventRepo>(repo: Arc<R>, state: Continuation) -> Result<()> {
    use meta_secret_core::node::{
        api::{ServerTailRequest, SsRequest},
        db::{
            descriptors::shared_secret_descriptor::{SsDeviceLogDescriptor, SsLogDescriptor},
            objects::persistent_shared_secret::PersistentSharedSecret,
        },
    };
    let original_raw_present = repo.find_one(state.source.obj_id()).await?.is_some();
    let audited = Arc::new(IntakeFaultRepo::new(repo.clone()));
    if original_raw_present {
        audited.arm(Phase::Raw, Fault::Before);
    }
    let server = ServerApp::new(audited.clone(), state.master)?;
    let before_signature = canonical_json(&state.signed)?;
    let tail = SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(ServerTailRequest {
        producer_reference: None,
        sender: state.a.user(),
    })));
    assert_clean(&server.handle_client_request(tail).await?)?;
    if repo.find_one(state.source.obj_id()).await?.is_none() {
        // No Prepared ever committed: the SAME still-fresh signature is an ordinary retry.
        send(&server, state.signed.clone()).await?;
    }
    let p = Arc::new(PersistentObject::new(repo.clone()));
    let log = PersistentSharedSecret::from(p.clone())
        .find_ss_log_tail_event(state.a.vault_name.clone())
        .await?
        .context("same original canonical claim")?;
    assert_eq!(log.obj_id(), state.target);
    assert!(log.0.value.pending_recovery_request_intake.is_none());
    assert!(log.0.value.claims.get(&state.source.0.value.id) == Some(&state.source.0.value));
    assert!(log.0.value.liveboard_recovery_results.is_empty());
    assert!(repo.find_one(state.source.obj_id()).await? == Some(state.source.clone().to_generic()));
    let raws = p
        .get_object_events_from_beginning(SsDeviceLogDescriptor::from(state.a.device_id().clone()))
        .await?;
    assert_eq!(
        raws.len(),
        1,
        "no extra nonce, replacement Claim, Split or new command"
    );
    assert!(raws[0] == state.source);
    assert!(
        PersistentVault::from(p)
            .get_vault(state.a.vault_name.clone())
            .await?
            == state.admission
    );
    if original_raw_present {
        assert_eq!(
            audited.hits.load(Ordering::SeqCst),
            0,
            "continuation must not rewrite an already equal original raw source"
        );
    }
    assert!(canonical_json(&state.signed)? == before_signature);
    state.signed.verify(&state.a.device().keys.dsa_pk)?;
    assert_clean(
        &server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                SsRequest {
                    sender: state.a.user(),
                    ss_log: ArtifactId::from(SsLogDescriptor::from(state.a.vault_name.clone())),
                },
            ))))
            .await?,
    )?;
    assert!(
        send(&server, state.signed).await.is_err(),
        "ordinary already-consumed Event replay stays rejected"
    );
    Ok(())
}
async fn fault(phase: Phase, kind: Fault) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let state = interrupt(inner.clone(), phase, kind).await?;
    reopen(inner, state).await
}
macro_rules! case {
    ($name:ident,$phase:ident,$fault:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            fault(Phase::$phase, Fault::$fault).await
        }
    };
}
case!(
    recovery_inflight_canonical_silent_drop_keeps_original_signature_fresh,
    Canonical,
    Drop
);
case!(
    recovery_inflight_canonical_lost_reply_repairs_captured_admission,
    Canonical,
    LostReply
);
case!(
    recovery_inflight_raw_before_reopens_same_prepared_source,
    Raw,
    Before
);
case!(
    recovery_inflight_raw_lost_reply_reuses_original_raw,
    Raw,
    LostReply
);
case!(
    recovery_inflight_cleanup_before_keeps_fixed_target,
    Cleanup,
    Before
);
case!(
    recovery_inflight_cleanup_lost_reply_never_rewrites_original_raw,
    Cleanup,
    LostReply
);
