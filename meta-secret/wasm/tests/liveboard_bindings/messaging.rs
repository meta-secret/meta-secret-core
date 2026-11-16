use super::{fault_repo::Fault, fixtures::*};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    app::meta_app::messaging::GenericAppStateRequest,
    common::model::{ApplicationState, meta_pass::PlainPassInfo},
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::vault::{device_log_event::DeviceLogObject, vault_log_event::*},
    },
};
use std::{
    pin::pin,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

#[tokio::test]
async fn checked_binding_enqueues_to_actual_service_owner_before_any_persistence() -> Result<()> {
    let h = Harness::new(1).await?;
    let service = Arc::new(h.service());
    let manager = meta_secret_wasm::app_manager::ApplicationManager::new(
        h.protocol.clone(),
        h.gateway(),
        service.clone(),
        h.master.clone(),
    );
    let before = h.p.repo.get_db().await;
    let operation = manager
        .cluster_distribution_checked(PlainPassInfo::new("owned-create".into(), VALUE.into()));
    let mut operation = pin!(operation);
    assert!(
        futures_lite::future::poll_once(operation.as_mut())
            .await
            .is_none(),
        "RED: checked manager must wait for the serial service, not run a direct side lane"
    );
    assert!(
        h.p.repo.get_db().await == before,
        "queueing may not bypass the service owner"
    );
    let queued: Vec<_> = service.data_transfer.dt.service_drain()?.collect();
    assert_eq!(
        queued.len(),
        1,
        "exactly one checked operation must be queued"
    );
    assert_eq!(queued[0].kind(), "ClusterDistributionChecked");
    for request in queued {
        service.data_transfer.dt.requeue_call(request)?;
    }
    tokio::select! {
        result = operation => {
            let receipt = result.context("service-owned checked operation must return its actual receipt")?;
            assert!(receipt.secret_id.is_some());
        },
        result = service.run() => { result?; anyhow::bail!("service exited before replying to checked caller") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("service messaging watchdog elapsed"),
    }
    Ok(())
}

#[tokio::test]
async fn overlapping_state_and_checked_create_return_their_own_result_after_actual_post_save_fault()
-> Result<()> {
    let h = Harness::new(1).await?;
    let service = Arc::new(h.service());
    let manager = mobile_common::app_manager::ApplicationManager::new(
        h.protocol.clone(),
        h.gateway(),
        service.clone(),
        h.master.clone(),
    );
    h.p.repo.arm(Fault::BeforeSsDeviceLog);
    let operations = async {
        let (state, created) = tokio::join!(
            service.send_request(GenericAppStateRequest::GetState),
            manager.cluster_distribution_checked(PlainPassInfo::new(
                "correlated-create".into(),
                VALUE.into()
            )),
        );
        assert!(
            matches!(state?, ApplicationState::Vault(_)),
            "GetState must receive ApplicationState, not an operation receipt"
        );
        let receipt =
            created.context("RED: post-persist checked receipt must reach the checked caller")?;
        assert_eq!(
            h.p.repo.hits.load(Ordering::SeqCst),
            1,
            "actual later persistence fault must run"
        );
        let events: Vec<DeviceLogObject> =
            h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
                .await?;
        let ids: Vec<_> = events
            .iter()
            .filter_map(|event| match &event.0.value {
                VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(add)) => {
                    Some(&add.meta_pass_id)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 1, "unknown result may not resubmit Create");
        assert_eq!(receipt.secret_id.as_deref(), Some(ids[0].id_str().as_str()));
        let safe = serde_json::to_string(&receipt)?;
        assert!(!safe.contains("PRIVATE-ERROR-CONTENT") && !safe.contains(VALUE));
        anyhow::Ok(())
    };
    tokio::select! {
        result = operations => result,
        result = service.run() => { result?; anyhow::bail!("service exited before both callers completed") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("overlapping service messaging watchdog elapsed"),
    }
}
