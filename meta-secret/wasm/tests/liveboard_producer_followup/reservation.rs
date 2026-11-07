//! Existing reserved slots and pre-U captured admission remain fail-closed.
use super::{fixture::*, repo::Fault};
use anyhow::{Context, Result};
use meta_secret_core::{
    node::{
        api::{SyncRequest, WriteSyncRequest},
        common::model::{
            liveboard_producer::*,
            meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
        },
        db::{
            descriptors::{
                local_removal_envelope::LocalRemovalEnvelopeDescriptor,
                vault_descriptor::{DeviceLogDescriptor, VaultDescriptor},
            },
            events::{generic_log_event::*, vault::vault_log_event::*},
            in_mem_db::InMemKvLogEventRepo,
            objects::{
                persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
            },
            repo::generic_db::*,
        },
        security::sign_event,
    },
    secret::MetaDistributor,
};
use std::sync::{Arc, atomic::Ordering};

async fn reserved(fault: Fault, decision: bool) -> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.arm(fault);
        let receipt = h.remove(false).await?;
        assert_eq!(h.p.repo.fault_hits.load(Ordering::SeqCst), 1);
        let source = if decision {
            let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage
            else {
                anyhow::bail!("actual reserved U");
            };
            source.clone()
        } else {
            receipt.request_stage.source.clone()
        };
        assert!(
            h.p.repo
                .inner
                .find_one(source.event_id.clone())
                .await?
                .is_none()
        );
        let descriptor = LocalRemovalEnvelopeDescriptor(h.actor.user_id());
        let before =
            h.p.find_tail_event(descriptor.clone())
                .await?
                .context("actual pre-body durable reservation")?;
        assert!(before.0.value.producer_context.is_some());
        let writes = h.p.repo.writes.load(Ordering::SeqCst);
        let network = h.network.counts();
        let legacy = PersistentDeviceLog::from(h.p.clone())
            .save_add_meta_pass_request(AddMetaPassEvent {
                sender: h.actor.user().into(),
                meta_pass_id: MetaPasswordId::build_from_str(
                    "different-legacy-body-at-reserved-slot",
                ),
            })
            .await;
        assert!(
            legacy.is_err(),
            "different legacy body cannot consume active producer reservation"
        );
        assert!(h.p.repo.inner.find_one(source.event_id).await?.is_none());
        assert!(h.p.find_tail_event(descriptor).await? == Some(before));
        assert_eq!(h.p.repo.writes.load(Ordering::SeqCst), writes);
        assert_eq!(h.network.counts(), network);
        Ok(())
    })
    .await
}
#[tokio::test]
async fn legacy_append_cannot_replace_actual_reserved_r_after_before_body_failure_control()
-> Result<()> {
    reserved(Fault::RequestBefore, false).await
}
#[tokio::test]
async fn legacy_append_cannot_replace_actual_reserved_u_after_before_body_failure_control()
-> Result<()> {
    reserved(Fault::DecisionBefore, true).await
}

#[tokio::test]
async fn pre_u_admission_revision_changed_by_actual_peer_create_stays_rejected_control()
-> Result<()> {
    let h = Harness::new(Arc::new(InMemKvLogEventRepo::default()), 2).await?;
    drive(&h, async {
        h.web.get_state().await?;
        h.p.repo.arm(Fault::DecisionBefore);
        let receipt = h.remove(false).await?;
        let descriptor = LocalRemovalEnvelopeDescriptor(h.actor.user_id());
        let context =
            h.p.find_tail_event(descriptor.clone())
                .await?
                .and_then(|event| event.0.value.producer_context)
                .context("actual reserved pre-U context")?;
        let Some(ProducerDecisionStage::Attempted { source, .. }) = &receipt.decision_stage else {
            anyhow::bail!("actual reserved U");
        };
        assert!(
            h.p.repo
                .inner
                .find_one(source.event_id.clone())
                .await?
                .is_none()
        );
        // The actual other current member makes a legitimate new Create, advancing Vault revision.
        let peer_local = Arc::new(PersistentObject::in_mem());
        for record in h.network.repo.get_db().await.into_values() {
            peer_local.repo.save(record).await?;
        }
        let vault = peer_local
            .find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
            .await?
            .context("actual current peer canonical Vault")?;
        let vm = vault.to_data().to_vault_member(h.target.user().into())?;
        MetaDistributor {
            p_obj: peer_local.clone(),
            user_creds: Arc::new(h.target.clone()),
            vault_member: vm.clone(),
        }
        .distribute_checked(
            vm,
            SecurePassInfo::from(PlainPassInfo::new(
                "legitimate-peer-revision".into(),
                "SYNTHETIC-PEER-SECRET".into(),
            )),
        )
        .await?;
        let actual = peer_local
            .find_tail_event(DeviceLogDescriptor::from(h.target.user_id()))
            .await?
            .context("actual producer-saved peer AddMetaPass")?;
        let signed = sign_event(
            actual.to_generic(),
            h.target.device_id().clone(),
            &h.target.device_creds.key_manager()?.dsa,
        )?;
        h.network
            .server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed,
            ))))
            .await?
            .ensure_success()?;
        h.web.get_state().await?;
        let current =
            h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
                .await?
                .context("actual refreshed changed admission")?;
        assert_eq!(current.0.value.members().len(), 2);
        assert_ne!(current.obj_id(), context.admission_vault);
        h.p.repo.reset_observation();
        let before = h.p.find_tail_event(descriptor.clone()).await?;
        let network = h.network.counts();
        let writes = h.p.repo.writes.load(Ordering::SeqCst);
        let result = h.resume(false, reference(&receipt)).await?;
        assert_eq!(result.phase, ProducerPhase::Unknown);
        assert!(
            h.p.repo
                .inner
                .find_one(source.event_id.clone())
                .await?
                .is_none()
        );
        assert!(h.p.find_tail_event(descriptor).await? == before);
        assert!(h.p.repo.sources().is_empty());
        assert_eq!(h.p.repo.material_attempts.load(Ordering::SeqCst), 0);
        assert_eq!(h.p.repo.writes.load(Ordering::SeqCst), writes);
        assert_eq!(h.network.counts().0, network.0);
        Ok(())
    })
    .await
}
