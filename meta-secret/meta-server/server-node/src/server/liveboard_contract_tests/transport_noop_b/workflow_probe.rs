//! A real positive reader on the exact snapshot selects the last addressed item;
//! the production contract does not mandate incidental HashMap iteration order.
use super::{repository::Repo, *};
use meta_secret_core::node::{
    api::SsRequest, db::descriptors::shared_secret_descriptor::SsLogDescriptor,
};

pub(super) async fn delivery_order(
    h: &Seed<Repo<InMemKvLogEventRepo>>,
    direct: bool,
    expected: &[ArtifactId],
    from: Option<ArtifactId>,
) -> Result<Vec<ArtifactId>> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    for event in h.repo.inner.get_db().await.into_values() {
        inner.save(event).await?;
    }
    let probe = Arc::new(Repo::new(inner));
    let server = ServerApp::new(probe.clone(), h.master.clone())?;
    server.init().await?;
    let p = Arc::new(PersistentObject::new(probe.clone()));
    let gateway = crate::server::server_data_sync::ServerSyncGateway::from(p);
    let request = SsRequest {
        sender: h.actor.user(),
        ss_log: from
            .unwrap_or_else(|| ArtifactId::from(SsLogDescriptor::from(h.actor.vault_name.clone()))),
    };
    probe.reads.lock().unwrap().clear();
    let delivered = if direct {
        gateway
            .ss_replication(request, server.init().await?.device.device_id)
            .await?
    } else {
        server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                request,
            ))))
            .await?
            .to_data()?
            .0
    };
    for id in expected {
        assert!(
            delivered.iter().any(|event| event.obj_id() == *id),
            "real healthy baseline must deliver each addressed encrypted workflow"
        );
    }
    let mut order = vec![];
    for id in probe.reads.lock().unwrap().iter() {
        if expected.contains(id) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    assert_eq!(order.len(), expected.len());
    eprintln!(
        "actual healthy delivery baseline: direct={direct} ordered_count={}",
        order.len()
    );
    Ok(order)
}
