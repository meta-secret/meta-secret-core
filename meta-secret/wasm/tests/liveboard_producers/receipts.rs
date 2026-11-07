use super::{fixture::*, repo::Fault};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    common::model::liveboard_producer::*,
    db::{
        events::{generic_log_event::*, vault::vault_log_event::*},
        in_mem_db::InMemKvLogEventRepo,
        objects::{persistent_object::PersistentObject, persistent_vault::PersistentVault},
        repo::generic_db::*,
    },
};
use std::sync::{Arc, atomic::Ordering};

async fn deletion(mobile: bool, fault: Option<Fault>) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h,async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        if let Some(fault)=fault {h.p.repo.arm(fault);}
        let receipt=h.delete(mobile,h.primary.clone()).await.context("RED: actual checked Delete producer unavailable")?;
        let sources=h.p.repo.sources(); let source=first_request(&sources)?;
        assert_source(&receipt,source)?;
        assert_eq!(receipt.kind,ProducerKind::DeleteSecret);
        assert_eq!(receipt.secret_id.as_deref(),Some(h.primary.id_str().as_str()));
        assert!(receipt.decision_stage.is_none() && receipt.material_stage.is_none());
        assert_eq!(sources.iter().filter(|s|matches!(s.0.value,VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(_)))).count(),1);
        if fault.is_none() {
            assert_eq!(receipt.phase,ProducerPhase::Queued);
            assert_eq!(receipt.canonical_state,ProducerCanonicalState::Pending);
            assert_eq!(receipt.request_stage.observation,ProducerSourceObservation::RawAcknowledged);
            let p=Arc::new(PersistentObject::new(h.network.repo.clone()));
            let log=PersistentVault::from(p).get_vault_log_artifact(h.actor.vault_name.clone()).await?;
            assert!(log.0.value.requests.iter().any(|r|matches!(r,VaultActionRequestEvent::DeleteSecret(actual) if Some(&actual.request_id)==receipt.request_id.as_ref())));
        } else {
            assert_eq!(h.p.repo.fault_hits.load(Ordering::SeqCst),1);
            if h.p.repo.inner.find_one(source.obj_id()).await?.is_none() {assert_eq!(receipt.phase,ProducerPhase::Unknown);}
        }
        Ok(())
    }).await
}
async fn removal(mobile: bool) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        let receipt = h
            .remove(mobile)
            .await
            .context("RED: actual checked Remove producer unavailable")?;
        let sources = h.p.repo.sources();
        let source = first_request(&sources)?;
        assert_source(&receipt, source)?;
        assert_eq!(receipt.kind, ProducerKind::RemoveDevice);
        assert_eq!(
            receipt.target_device_id.as_deref(),
            Some(h.target.device_id().to_string().as_str())
        );
        assert!(receipt.secret_id.is_none());
        assert!(receipt.decision_stage.is_none() && receipt.material_stage.is_none());
        assert_eq!(
            sources.len(),
            1,
            "remote three-device Remove has R only; no local U"
        );
        let p = Arc::new(PersistentObject::new(h.network.repo.clone()));
        let log = PersistentVault::from(p)
            .get_vault_log_artifact(h.actor.vault_name.clone())
            .await?;
        let request = match &source.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) => r,
            _ => anyhow::bail!("actual Remove R"),
        };
        let binding = log
            .0
            .value
            .liveboard_removal_pending
            .iter()
            .find(|b| b.request == *request)
            .context("exact admitted source's actual canonical P")?;
        assert_eq!(receipt.request_id.as_ref(), Some(&binding.request_id));
        assert_eq!(receipt.canonical_state, ProducerCanonicalState::Pending);
        assert_eq!(
            receipt.request_stage.observation,
            ProducerSourceObservation::RawAcknowledged
        );
        Ok(())
    })
    .await
}
#[tokio::test]
async fn web_delete_actual_uuid_source_commitment_and_pending_receipt() -> Result<()> {
    deletion(false, None).await
}
#[tokio::test]
async fn mobile_delete_actual_uuid_source_commitment_and_pending_receipt() -> Result<()> {
    deletion(true, None).await
}
#[tokio::test]
async fn web_remove_actual_server_admitted_occurrence_and_source_only() -> Result<()> {
    removal(false).await
}
#[tokio::test]
async fn mobile_remove_actual_server_admitted_occurrence_and_source_only() -> Result<()> {
    removal(true).await
}
#[tokio::test]
async fn web_delete_before_append_retains_actual_reserved_uuid_source_unknown() -> Result<()> {
    deletion(false, Some(Fault::RequestBefore)).await
}
#[tokio::test]
async fn mobile_delete_lost_append_reply_retains_same_actual_uuid_source() -> Result<()> {
    deletion(true, Some(Fault::RequestLostReply)).await
}

#[tokio::test]
async fn actual_signed_legacy_delete_fixture_is_a_positive_business_control() -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 3).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.reset_observation();
        h.service.delete_secret(h.primary.clone()).await?;
        let sources = h.p.repo.sources();
        let source = first_request(&sources)?;
        let p = Arc::new(PersistentObject::new(h.network.repo.clone()));
        let log = PersistentVault::from(p)
            .get_vault_log_artifact(h.actor.vault_name.clone())
            .await?;
        assert!(
            log.0
                .value
                .requests
                .iter()
                .any(|r| matches!((&source.0.value,r),(VaultActionEvent::Request(a),b) if a==b))
        );
        assert!(
            h.network.repo.find_one(source.obj_id()).await? == Some(source.clone().to_generic())
        );
        Ok(())
    })
    .await
}
