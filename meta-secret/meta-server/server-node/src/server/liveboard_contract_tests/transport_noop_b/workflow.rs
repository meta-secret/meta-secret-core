//! Deliberately corrupted records exercise real readers; no fake signed actor/handler.
use super::{
    repository::Repo,
    support::{normal, project_initial_split_records},
    *,
};
use meta_secret_core::node::{
    api::SsRequest,
    common::model::{
        meta_pass::{PlainPassInfo, SecurePassInfo},
        secret::*,
        vault::vault::VaultMember,
    },
    db::{
        descriptors::shared_secret_descriptor::*, events::shared_secret_event::*,
        objects::persistent_shared_secret::PersistentSharedSecret,
    },
};
use meta_secret_core::secret::MetaDistributor;
use std::sync::atomic::Ordering;

async fn run(direct: bool, kind: u8, two: bool) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(Repo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 2).await?;
    let publisher = Arc::new(super::super::recovery_intake_fixture::Publisher::default());
    let server = ServerApp::new_with_invalidation_publisher(
        repo.clone(),
        h.master.clone(),
        publisher.clone(),
    )?;
    server.init().await?;
    if two {
        let body = normal(&h, &server).await?;
        let VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(add)) = body.0.value
        else {
            unreachable!()
        };
        let vault = PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?;
        let vm = VaultMember {
            member: member(&h.actor),
            vault: vault.0.value,
        };
        MetaDistributor {
            p_obj: h.p.clone(),
            user_creds: Arc::new(h.actor.clone()),
            vault_member: vm.clone(),
        }
        .distribute(
            vm,
            SecurePassInfo::from(PlainPassInfo {
                pass_id: add.meta_pass_id,
                pass: "second real encrypted distribution".into(),
            }),
        )
        .await?;
    }
    project_initial_split_records(&h).await?;
    let log = PersistentSharedSecret::from(h.p.clone())
        .find_ss_log_tail_event(h.actor.vault_name.clone())
        .await?
        .context("actual split claims")?;
    let ordered: Vec<_> = log.0.value.claims.values().collect();
    assert_eq!(ordered.len(), if two { 2 } else { 1 });
    let expected: Vec<_> = ordered
        .iter()
        .map(|claim| {
            ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: claim.dist_claim_id.pass_id.clone(),
                receiver: h.actor.device_id().clone(),
            }))
        })
        .collect();
    let baseline = super::workflow_probe::delivery_order(&h, direct, &expected, None).await?;
    let id = baseline
        .last()
        .context("actual last addressed delivery")?
        .clone();
    let claim = ordered
        .iter()
        .find(|claim| {
            ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: claim.dist_claim_id.pass_id.clone(),
                receiver: h.actor.device_id().clone(),
            })) == id
        })
        .context("exact baseline-selected real claim")?;
    let original = inner
        .find_one(id.clone())
        .await?
        .context("actual addressed encrypted distribution")?;
    let GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(mut packet)) = original else {
        anyhow::bail!("real distribution fixture expected")
    };
    let corrupt = match kind {
        0 => SsWorkflowObject::Recovery(packet).to_generic(),
        1 => {
            packet.value.vault_name = "foreign-workflow-vault".into();
            SsWorkflowObject::Distribution(packet).to_generic()
        }
        2 => {
            packet.value.claim_id.pass_id =
                meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str(
                    "wrong pass",
                );
            SsWorkflowObject::Distribution(packet).to_generic()
        }
        3 => {
            let other = FixtureRegistry::empty().state.user_creds.client;
            packet.key.obj_desc=meta_secret_core::node::db::descriptors::object_descriptor::ToObjectDescriptor::to_obj_desc(SsWorkflowDescriptor::Distribution(SsDistributionId{pass_id:claim.dist_claim_id.pass_id.clone(),receiver:other.device_id().clone()}));
            SsWorkflowObject::Distribution(packet).to_generic()
        }
        5 => {
            let mut peer_value = None;
            for workflow in &h.workflows {
                let value = inner
                    .find_one(workflow.clone())
                    .await?
                    .context("actual peer encrypted packet")?
                    .shared_secret()?
                    .to_distribution_data()?;
                if value
                    .secret_message
                    .cipher_text()
                    .channel
                    .receiver()
                    .to_device_id()
                    != *h.actor.device_id()
                {
                    peer_value = Some(value.secret_message);
                    break;
                }
            }
            packet.value.secret_message =
                peer_value.context("actual cipher receiver peer, not requesting A")?;
            SsWorkflowObject::Distribution(packet).to_generic()
        }
        _ => {
            packet.key.obj_id = id.clone().next();
            SsWorkflowObject::Distribution(packet).to_generic()
        }
    };
    inner.db.lock().await.insert(id.clone(), corrupt);
    let before = inner.get_db().await;
    let writes = repo.writes.load(Ordering::SeqCst);
    let published = publisher.0.load(Ordering::SeqCst);
    let request = SsRequest {
        sender: h.actor.user(),
        ss_log: ArtifactId::from(SsLogDescriptor::from(h.actor.vault_name.clone())),
    };
    let gateway =
        crate::server::server_data_sync::ServerSyncGateway::new(h.p.clone(), publisher.clone());
    repo.reads.lock().unwrap().clear();
    let result = if direct {
        gateway
            .ss_replication(request, server.init().await?.device.device_id)
            .await
            .map(|_| ())
    } else {
        server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                request,
            ))))
            .await
            .map(|_| ())
    };
    let unchanged = inner.get_db().await == before;
    if two && !unchanged {
        let reads = repo.reads.lock().unwrap();
        let healthy = reads
            .iter()
            .position(|id| id == &baseline[0])
            .context("actual healthy first lookup reached")?;
        let bad = reads
            .iter()
            .position(|found| found == &id)
            .context("actual corrupt second lookup reached")?;
        anyhow::ensure!(
            healthy < bad,
            "fixture must demonstrate actual healthy-first/corrupt-second baseline, not incidental HashMap order"
        );
    }
    eprintln!(
        "actual workflow reader: direct={direct} kind={kind} two={two} db_unchanged={unchanged} writes={} returned_error={}",
        repo.writes.load(Ordering::SeqCst) - writes,
        result.is_err()
    );
    anyhow::ensure!(
        unchanged,
        "invalid addressed set caused partial delete/SsLog status writes before validation"
    );
    anyhow::ensure!(
        repo.writes.load(Ordering::SeqCst) == writes,
        "addressed validation must precede every write/delete"
    );
    anyhow::ensure!(
        publisher.0.load(Ordering::SeqCst) == published,
        "invalid addressed set published invalidation"
    );
    anyhow::ensure!(
        result.is_err(),
        "invalid addressed key/subtype/Vault/pass/recipient must reject, not skip or publish"
    );
    Ok(())
}
macro_rules! wf_case {
    ($name:ident,$kind:expr) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            let outer = run(false, $kind, false).await;
            let direct = run(true, $kind, false).await;
            outer?;
            direct
        }
    };
}
wf_case!(
    noop_b_addressed_distribution_wrong_subtype_no_delivery_effect,
    0
);
wf_case!(
    noop_b_addressed_distribution_wrong_vault_no_delivery_effect,
    1
);
wf_case!(
    noop_b_addressed_distribution_wrong_pass_no_delivery_effect,
    2
);
wf_case!(
    noop_b_addressed_distribution_wrong_recipient_descriptor_no_delivery_effect,
    3
);
wf_case!(
    noop_b_addressed_distribution_wrong_inner_key_cannot_be_silently_skipped,
    4
);
wf_case!(
    noop_b_addressed_distribution_actual_peer_cipher_at_correct_own_key_no_delivery_effect,
    5
);
#[tokio::test]
async fn noop_b_valid_first_bad_second_addressed_set_is_atomic() -> Result<()> {
    let outer = run(false, 0, true).await;
    let direct = run(true, 0, true).await;
    outer?;
    direct
}

async fn fallback(direct: bool) -> Result<()> {
    let inner = Arc::new(InMemKvLogEventRepo::default());
    let repo = Arc::new(Repo::new(inner.clone()));
    let h = Seed::new(repo.clone(), 2).await?;
    let server = h.server().await?;
    project_initial_split_records(&h).await?;
    let mut log = PersistentSharedSecret::from(h.p.clone())
        .find_ss_log_tail_event(h.actor.vault_name.clone())
        .await?
        .context("actual clean canonical SsLog")?;
    for claim in log.0.value.claims.values_mut() {
        claim.vault_name = "foreign-clean-fallback-vault".into();
    }
    assert!(log.0.value.pending_recovery_request_intake.is_none());
    let start = log.obj_id().next().next();
    let expected: Vec<_> = log
        .0
        .value
        .claims
        .values()
        .map(|claim| {
            ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: claim.dist_claim_id.pass_id.clone(),
                receiver: h.actor.device_id().clone(),
            }))
        })
        .collect();
    // Healthy beyond-head fallback executes first on the exact uncorrupted snapshot.
    let mut clean = log.clone();
    for claim in clean.0.value.claims.values_mut() {
        claim.vault_name = h.actor.vault_name.clone();
    }
    inner
        .db
        .lock()
        .await
        .insert(clean.obj_id(), clean.to_generic());
    super::workflow_probe::delivery_order(&h, direct, &expected, Some(start.clone())).await?;

    inner.db.lock().await.insert(log.obj_id(), log.to_generic());
    let before = inner.get_db().await;
    let writes = repo.writes.load(Ordering::SeqCst);
    let request = SsRequest {
        sender: h.actor.user(),
        ss_log: start,
    };
    let gateway = crate::server::server_data_sync::ServerSyncGateway::from(h.p.clone());
    let result = if direct {
        gateway
            .ss_replication(request, server.init().await?.device.device_id)
            .await
            .map(|_| ())
    } else {
        server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                request,
            ))))
            .await
            .map(|_| ())
    };
    assert!(
        inner.get_db().await == before,
        "wrong-context beyond-head fallback caused effects"
    );
    assert_eq!(repo.writes.load(Ordering::SeqCst), writes);
    assert!(
        result.is_err(),
        "clean wrong-inner-Vault fallback bypassed typed SsLog guard"
    );
    Ok(())
}
#[tokio::test]
async fn noop_b_outer_beyond_head_clean_sslog_fallback_validates_context_before_effects()
-> Result<()> {
    fallback(false).await
}
#[tokio::test]
async fn noop_b_direct_beyond_head_clean_sslog_fallback_validates_context_before_effects()
-> Result<()> {
    fallback(true).await
}
