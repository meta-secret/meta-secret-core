//! A direct public gateway must guard dirty canonical payload even with no raw source yet.
use super::{
    correlation_faults::{Fault, Step, transfer_count},
    producer_guard_fixture::GuardFixture,
    *,
};
use meta_secret_core::node::db::repo::generic_db::FindOneQuery;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn direct_gateway_read_cannot_expose_deleted_transfer_from_fresh_admission_raw_gap()
-> Result<()> {
    let h = GuardFixture::new(false).await?;
    // Actual source was queued before Delete; its first valid admission happens after tombstone.
    h.seed.repo.arm(&h.source, "", Step::Raw, Fault::Before);
    assert!(send(&h.server, h.signed.clone()).await.is_err());
    assert_eq!(h.seed.repo.hits.load(Ordering::SeqCst), 1);
    h.seed.repo.clear();
    assert!(
        h.seed
            .repo
            .inner
            .find_one(h.source.obj_id())
            .await?
            .is_none()
    );
    let original_ref = h.retained_ref().await?;
    let canonical = PersistentVault::from(h.seed.p.clone())
        .get_vault_log_artifact(h.seed.actor.vault_name.clone())
        .await?;
    assert!(
        transfer_count(&serde_json::to_value(&canonical)?) > 0,
        "actual pending+verified-ref admission must retain the original transfer at this gap"
    );
    let gateway = super::super::server_data_sync::ServerSyncGateway::new(
        h.seed.p.clone(),
        h.publisher.clone(),
    );
    let request = VaultRequest {
        sender: h.seed.actor.user(),
        tail: VaultTail {
            vault_status: ArtifactId::from(VaultStatusDescriptor::from(h.seed.actor.user_id())),
            vault_log: ArtifactId::from(VaultLogDescriptor::from(h.seed.actor.vault_name.clone())),
            vault: ArtifactId::from(VaultDescriptor::from(h.seed.actor.vault_name.clone())),
        },
    };
    // Deliberately direct: do not let outer ServerApp maintenance hide this public-route gap.
    if let Ok(events) = gateway.vault_replication(request.clone()).await {
        assert!(!events.is_empty(), "real generic replication path");
        assert_eq!(
            transfer_count(&serde_json::to_value(events)?),
            0,
            "RED: direct gateway exported deleted encrypted transfer from canonical pending/binding while raw absent"
        );
    }
    assert_eq!(h.retained_ref().await?, original_ref);
    let outer = h
        .server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(request))))
        .await?;
    assert_eq!(transfer_count(&serde_json::to_value(outer)?), 0);
    assert!(
        h.seed
            .repo
            .inner
            .find_one(h.source.obj_id())
            .await?
            .is_none(),
        "read repair must not recreate raw source or grant replay rights"
    );
    assert_eq!(h.retained_ref().await?, original_ref);
    Ok(())
}
