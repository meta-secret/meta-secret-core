//! One physical current-value packet must not be delivered once per retained claim.
use super::{integration_binding_fixture::Seed, *};
use async_trait::async_trait;
use meta_secret_core::{
    node::{
        api::SsRequest,
        common::model::{
            meta_pass::{PlainPassInfo, SecurePassInfo},
            secret::*,
            vault::vault::VaultMember,
        },
        db::{
            descriptors::shared_secret_descriptor::*,
            events::generic_log_event::*,
            objects::persistent_shared_secret::PersistentSharedSecret,
            repo::{generic_db::*, recovery_response_store::ServerRecoveryResponseStore},
        },
    },
    secret::MetaDistributor,
};
use std::sync::Mutex;

#[derive(Default)]
struct Repo {
    inner: InMemKvLogEventRepo,
    deleted: Mutex<Vec<ArtifactId>>,
}
#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        self.inner.find_one(id).await
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.save(value).await
    }
    async fn replace<T: ToGenericEvent>(&self, value: T) -> Result<ArtifactId> {
        self.inner.replace(value).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.deleted.lock().unwrap().push(id.clone());
        self.inner.delete(id).await;
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.deleted.lock().unwrap().push(id.clone());
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(&self) -> Result<&dyn ServerRecoveryResponseStore> {
        self.inner.server_recovery_response_store()
    }
}

async fn read(
    h: &Seed<Repo>,
    server: &ServerApp<Repo>,
    direct: bool,
    receiver: meta_secret_core::node::common::model::user::common::UserData,
) -> Result<Vec<GenericKvLogEvent>> {
    let request = SsRequest {
        sender: receiver,
        ss_log: ArtifactId::from(SsLogDescriptor::from(h.actor.vault_name.clone())),
    };
    if direct {
        crate::server::server_data_sync::ServerSyncGateway::from(h.p.clone())
            .ss_replication(request, server.init().await?.device.device_id)
            .await
    } else {
        Ok(server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                request,
            ))))
            .await?
            .to_data()?
            .0)
    }
}
async fn run(direct: bool) -> Result<()> {
    let h = Seed::new(Arc::new(Repo::default()), 2).await?;
    let server = h.server().await?;
    let shared = PersistentSharedSecret::from(h.p.clone());
    let current_vault = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    let receiver = current_vault
        .0
        .value
        .members()
        .into_iter()
        .map(|member| member.user_data)
        .find(|user| user.device.device_id != *h.actor.device_id())
        .context("actual canonical peer receiver with Pending status")?;
    let original = shared
        .find_ss_device_log_tail_event(h.actor.device_id().clone())
        .await?
        .context("actual first Core Split")?;
    shared.save_ss_log_event(original.0.value.clone()).await?; // Initial canonical fixture projection, no fake handler.
    let packet_id = ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: h.pass.clone(),
        receiver: receiver.device.device_id.clone(),
    }));
    let first = read(&h, &server, direct, receiver.clone()).await?;
    assert_eq!(
        first
            .iter()
            .filter(|event| event.obj_id() == packet_id)
            .count(),
        1
    );
    assert!(h.repo.find_one(packet_id.clone()).await?.is_none());
    let delivered = shared
        .find_ss_log_tail_event(h.actor.vault_name.clone())
        .await?
        .context("actual first delivery completion")?;
    assert_eq!(
        delivered.0.value.claims[&original.0.value.id]
            .status
            .get(&receiver.device.device_id),
        Some(&SsDistributionStatus::Delivered)
    );
    assert!(
        delivered.0.value.claims.contains_key(&original.0.value.id),
        "real delivered history must remain retained"
    );

    let vault = PersistentVault::from(h.p.clone())
        .get_vault(h.actor.vault_name.clone())
        .await?;
    let member = VaultMember {
        member: member(&h.actor),
        vault: vault.0.value,
    };
    MetaDistributor {
        p_obj: h.p.clone(),
        user_creds: Arc::new(h.actor.clone()),
        vault_member: member.clone(),
    }
    .distribute(
        member,
        SecurePassInfo::from(PlainPassInfo {
            pass_id: h.pass.clone(),
            pass: "SYNTHETIC-PRIVATE-DELETE-SECRET".into(),
        }),
    )
    .await?;
    let next = shared
        .find_ss_device_log_tail_event(h.actor.device_id().clone())
        .await?
        .context("actual second same-Pass Core Split")?;
    assert_ne!(next.0.value.id, original.0.value.id);
    assert_eq!(
        next.0.value.dist_claim_id.pass_id,
        original.0.value.dist_claim_id.pass_id
    );
    shared.save_ss_log_event(next.0.value.clone()).await?;
    let current = h
        .repo
        .find_one(packet_id.clone())
        .await?
        .context("one actual current encrypted packet")?;
    assert_eq!(
        current
            .clone()
            .shared_secret()?
            .to_distribution_data()?
            .claim_id
            .id,
        next.0.value.id
    );
    let before = shared
        .find_ss_log_tail_event(h.actor.vault_name.clone())
        .await?
        .context("both actual retained claims")?;
    assert_eq!(before.0.value.claims.len(), 2);
    assert_eq!(
        before.0.value.claims[&next.0.value.id]
            .status
            .get(&receiver.device.device_id),
        Some(&SsDistributionStatus::Pending)
    );
    assert!(
        before
            .0
            .value
            .claims
            .values()
            .all(
                |claim| claim.distribution_type == SecretDistributionType::Split
                    && claim.dist_claim_id.pass_id == h.pass
            )
    );
    assert_eq!(
        before.0.value.claims[&original.0.value.id],
        delivered.0.value.claims[&original.0.value.id]
    );
    // Preserve the old reader's FIRST actually visited claim; do not select by generation/ID.
    let first_visited = before
        .0
        .value
        .claims
        .values()
        .next()
        .context("actual first retained entry")?
        .id
        .clone();
    let expected = before
        .0
        .value
        .clone()
        .complete(first_visited, receiver.device.device_id.clone());
    h.repo.deleted.lock().unwrap().clear();
    let events = read(&h, &server, direct, receiver.clone()).await?;
    let copies = events
        .iter()
        .filter(|event| event.obj_id() == packet_id)
        .count();
    let deletes = h
        .repo
        .deleted
        .lock()
        .unwrap()
        .iter()
        .filter(|id| **id == packet_id)
        .count();
    let after = shared
        .find_ss_log_tail_event(h.actor.vault_name.clone())
        .await?
        .context("actual post-delivery canonical state")?;
    eprintln!(
        "actual retained same-Pass reader: direct={direct} retained=2 physical=1 copies={copies} deletes={deletes} first_completion_unchanged={}",
        after.0.value == expected
    );
    anyhow::ensure!(
        copies == 1,
        "same physical current packet was delivered more than once for retained Split history"
    );
    anyhow::ensure!(
        deletes == 1,
        "same physical current packet was deleted more than once"
    );
    anyhow::ensure!(
        events
            .iter()
            .filter(|event| event.obj_id() == packet_id)
            .all(|event| *event == current),
        "delivery changed the actual current encrypted bytes"
    );
    anyhow::ensure!(
        h.repo.find_one(packet_id).await?.is_none(),
        "successful actual delivery must remove current packet"
    );
    anyhow::ensure!(
        after.0.value == expected,
        "dedup changed previous first-visited completion semantics or another retained claim"
    );
    Ok(())
}
#[tokio::test]
async fn retained_same_pass_split_claims_deliver_delete_once_and_complete_first_visited_only()
-> Result<()> {
    let outer = run(false).await;
    let direct = run(true).await;
    outer?;
    direct
}
