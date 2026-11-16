//! A verified admitted source is not a new business request after its P terminalizes.
use super::integration_binding_fixture::{FaultRepo, tail};
use super::*;
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn integration_binding_remove_admitted_r_raw_gap_then_decline_continues_without_p2()
-> Result<()> {
    let h = Harness::new().await?;
    let repo = Arc::new(FaultRepo::new(h.p_obj.repo.clone()));
    let server = ServerApp::new(repo.clone(), h.master_key.clone())?;
    server.init().await?;
    let source = next_event(
        &h.p_obj,
        &h.a,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
    )
    .await?;
    let signed = sign_event(
        source.clone().to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    repo.before_raw();
    assert!(send(&server, signed.clone()).await.is_err());
    assert_eq!(
        repo.hits.load(Ordering::SeqCst),
        1,
        "real raw-save gap must be reached"
    );
    assert!(repo.find_one(source.obj_id()).await?.is_none());
    let id = h.pending_id().await?;
    // Actual other eligible device responds while R has not reached its raw log.
    let response = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false))),
    )
    .await?;
    let response = sign_event(
        response.to_generic(),
        h.c.device_id().clone(),
        &h.c.device_creds.key_manager()?.dsa,
    )?;
    send(&server, response).await?;
    let before = h.log().await?;
    let winner = result_record(&before, &id)?.clone();
    assert_eq!(winner["outcome"], "declined");
    assert_eq!(
        winner["responderId"],
        serde_json::to_value(h.c.device_id())?
    );
    send(&server, signed.clone())
        .await
        .context("exact admitted fresh R must finish only its raw storage")?;
    let after = h.log().await?;
    assert_eq!(
        result_record(&after, &id)?,
        &winner,
        "continuation must preserve exact P1 winner"
    );
    assert!(
        after
            .get("liveboardRemovalPending")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty),
        "RED: retry of same admitted source must not allocate a second Pending P2 after Decline"
    );
    assert_eq!(
        after["liveboardRemovalResults"]
            .as_array()
            .context("results")?
            .len(),
        1
    );
    assert!(repo.find_one(source.obj_id()).await? == Some(source.clone().to_generic()));
    assert_eq!(
        tail(&server, &h.a).await?.device_log_tail,
        Some(source.obj_id().next())
    );
    assert!(
        send(&server, signed).await.is_err(),
        "stored original source remains ordinary replay-rejected"
    );
    assert_eq!(result_record(&h.log().await?, &id)?, &winner);
    Ok(())
}

#[tokio::test]
async fn integration_binding_remove_genuinely_new_source_after_decline_creates_new_occurrence_control()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let first = h.pending_id().await?;
    h.decide(false).await?;
    let terminal = result_record(&h.log().await?, &first)?.clone();
    h.request().await?;
    let second = h.pending_id().await?;
    assert_ne!(
        first, second,
        "only a genuinely new source may begin a new occurrence"
    );
    assert_eq!(result_record(&h.log().await?, &first)?, &terminal);
    Ok(())
}
