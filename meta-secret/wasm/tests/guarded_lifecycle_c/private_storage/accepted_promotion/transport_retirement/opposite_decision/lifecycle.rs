use super::*;
#[tokio::test]
async fn healthy_retired_decline_explicit_opposite_accept_reuses_pending_uuid_at_actual_next_nonce()
-> Result<()> {
    let h = harness::Harness::fresh(Arc::new(InMemKvLogEventRepo::default())).await?;
    let retired = proof::prerequisites(&h).await?;
    let queued = proof::queue(&h, retired).await?;
    proof::retry_and_admit(&h, &queued).await
}
