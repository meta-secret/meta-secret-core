use super::*;
pub(super) async fn startup<F: Future<Output = Result<()>>>(
    e: &fixture::Env,
    mut run: Pin<&mut F>,
    member: bool,
) -> Result<()> {
    e.repo.credential_owner_seen.store(false, Ordering::SeqCst);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(r) = run.as_mut().poll(cx) {
            return Poll::Ready(Err(r.err().unwrap_or_else(|| {
                anyhow::anyhow!("actual infinite service.run unexpectedly finished")
            })));
        }
        let (client, sync) = e.repo.owners();
        let entered = e.repo.credential_owner_seen.load(Ordering::SeqCst);
        if entered
            && !client
            && !sync
            && e.wire.tails() > 0
            && (if member {
                e.wire.ss() > 0
            } else {
                e.wire.ss() == 0
            })
        {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    })
    .await?;
    ensure!(
        matches!(
            e.service.data_transfer.dt.service_drain(),
            Err(DiagnosticTransferError::AlreadyOwned)
        ),
        "actual service receiver not Active"
    );
    let db = e.peer.physical.get_db().await;
    ensure!(db.values().any(|v|matches!(v,GenericKvLogEvent::VaultStatus(s)if match s.clone().status(){VaultStatus::Member(_)=>member,VaultStatus::Outsider(o)=>!member&&o.status==UserDataOutsiderStatus::Declined,_=>false})),"actual startup persisted state differs");
    eprintln!(
        "MOBILE_PRESYNC_RESET startup member={member} real_Tail={} real_SS={} owner_seen={entered} guardfree=true Active=true",
        e.wire.tails(),
        e.wire.ss(),
        entered = e.repo.credential_owner_seen.load(Ordering::SeqCst)
    );
    Ok(())
}
pub(super) async fn manager<F: Future<Output = Result<()>>>(
    e: &fixture::Env,
    mut run: Pin<&mut F>,
) -> Result<ApplicationState> {
    let mut call = Box::pin(e.manager.get_state());
    std::future::poll_fn(|cx| {
        if let Poll::Ready(r) = call.as_mut().poll(cx) {
            return Poll::Ready(r);
        }
        if let Poll::Ready(r) = run.as_mut().poll(cx) {
            return Poll::Ready(Err(r.err().unwrap_or_else(|| {
                anyhow::anyhow!("actual run unexpectedly completed during manager call")
            })));
        }
        call.as_mut().poll(cx)
    })
    .await
}
pub(super) async fn idle_poll<F: Future<Output = Result<()>>>(
    e: &fixture::Env,
    run: Pin<&mut F>,
) -> Result<()> {
    let a = e.repo.calls.lock().unwrap().len();
    let b = e.wire.calls.lock().unwrap().len();
    let result = once(run).await;
    ensure!(
        result.is_pending()
            && e.repo.calls.lock().unwrap().len() == a
            && e.wire.calls.lock().unwrap().len() == b,
        "error branch enqueued hidden normal FIFO GetState"
    );
    Ok(())
}
pub(super) fn closed(e: &fixture::Env) -> Result<()> {
    ensure!(
        matches!(
            e.service.data_transfer.dt.service_drain(),
            Err(DiagnosticTransferError::Closed)
        ),
        "actual run receiver not Closed after owned idle drop"
    );
    ensure!(
        e.repo.owners() == (false, false),
        "business owner held after service drain"
    );
    Ok(())
}
