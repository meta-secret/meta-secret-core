use super::{repository::Repo, support::*, *};
use std::{future::poll_fn, task::Poll};

async fn race(noop_first: bool) -> Result<()> {
    let repo = Arc::new(Repo::new(Arc::new(InMemKvLogEventRepo::default())));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    let (body, signed, uuid) = h.deletion().await?;
    let auth = signed_value(&h.actor, body.obj_id())?;
    let business = SyncRequest::Write(Box::new(WriteSyncRequest::Event(signed)));
    let noop = decode(&auth)?;
    let (first, second) = if noop_first {
        (noop, business)
    } else {
        (business, noop)
    };
    repo.pause_at(body.obj_id());
    let concurrent = async {
        repo.entered.notified().await;
        let second = server.handle_client_request(second);
        tokio::pin!(second);
        poll_fn(|cx| {
            assert!(
                second.as_mut().poll(cx).is_pending(),
                "competing same-N command bypassed actual server owner"
            );
            Poll::Ready(())
        })
        .await;
        repo.resume.notify_one();
        second.await
    };
    let (first, second) = tokio::join!(server.handle_client_request(first), concurrent);
    let first = first?;
    if noop_first {
        assert!(matches!(first, DataSyncResponse::DeviceTransportNoop(_)));
    } else {
        first.ensure_success()?;
    }
    assert!(
        second.is_err(),
        "losing same-N body/Noop must reject, never replace winner"
    );
    let stored = repo
        .inner
        .find_one(body.obj_id())
        .await?
        .context("actual winning slot")?;
    assert_eq!(
        matches!(stored, GenericKvLogEvent::DeviceTransportNoop(_)),
        noop_first
    );
    let vault = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    assert_eq!(
        vault.0.value.deletion_results.contains_key(&uuid),
        !noop_first
    );
    if noop_first {
        assert!(vault.0.value.secrets.contains(&h.pass));
    }
    assert_eq!(repo.inner.find_one(body.obj_id()).await?, Some(stored));
    Ok(())
}
#[tokio::test]
async fn noop_b_same_n_body_first_serializes_and_noop_cannot_override() -> Result<()> {
    race(false).await
}
#[tokio::test]
async fn noop_b_same_n_noop_first_serializes_and_late_body_never_dispatches() -> Result<()> {
    race(true).await
}
