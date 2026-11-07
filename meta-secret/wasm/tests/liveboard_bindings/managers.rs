use super::{
    fault_repo::{Fault, Repo},
    fixtures::*,
    transport::Protocol,
};
use anyhow::{Context, Result};
use meta_secret_core::node::{
    api::{CanonicalDecisionInput, SignedActionPayload},
    app::meta_app::meta_client_service::MetaClientService,
    common::model::{
        liveboard::LiveboardOperationReceipt,
        meta_pass::{MetaPasswordId, PlainPassInfo},
        user::common::*,
    },
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::vault::{device_log_event::DeviceLogObject, vault_log_event::*},
        repo::generic_db::*,
    },
};
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

enum Manager {
    Web(meta_secret_wasm::app_manager::ApplicationManager<Repo, Protocol>),
    Mobile(mobile_common::app_manager::ApplicationManager<Repo, Protocol>),
}
impl Manager {
    fn new(h: &Harness, web: bool) -> Self {
        let service = Arc::new(h.service());
        if web {
            Self::Web(meta_secret_wasm::app_manager::ApplicationManager::new(
                h.protocol.clone(),
                h.gateway(),
                service,
                h.master.clone(),
            ))
        } else {
            Self::Mobile(mobile_common::app_manager::ApplicationManager::new(
                h.protocol.clone(),
                h.gateway(),
                service,
                h.master.clone(),
            ))
        }
    }
    async fn create(&self, input: PlainPassInfo) -> Result<LiveboardOperationReceipt> {
        match self {
            Self::Web(m) => {
                execute(
                    &m.meta_client_service,
                    m.cluster_distribution_checked(input),
                )
                .await
            }
            Self::Mobile(m) => {
                execute(
                    &m.meta_client_service,
                    m.cluster_distribution_checked(input),
                )
                .await
            }
        }
    }
    async fn recover(&self, pass: MetaPasswordId) -> Result<LiveboardOperationReceipt> {
        match self {
            Self::Web(m) => m.recover_checked(pass).await,
            Self::Mobile(m) => m.recover_checked(pass).await,
        }
    }
    fn service(&self) -> &MetaClientService<Repo, Protocol> {
        match self {
            Self::Web(m) => &m.meta_client_service,
            Self::Mobile(m) => &m.meta_client_service,
        }
    }
    async fn remove(&self, input: CanonicalDecisionInput) -> Result<LiveboardOperationReceipt> {
        match self {
            Self::Web(m) => execute(&m.meta_client_service, m.decide_remove_member(input)).await,
            Self::Mobile(m) => execute(&m.meta_client_service, m.decide_remove_member(input)).await,
        }
    }
    async fn join(&self, input: CanonicalDecisionInput) -> Result<LiveboardOperationReceipt> {
        match self {
            Self::Web(m) => execute(&m.meta_client_service, m.decide_join_request(input)).await,
            Self::Mobile(m) => execute(&m.meta_client_service, m.decide_join_request(input)).await,
        }
    }
}

async fn exact_current_join(web: bool) -> Result<()> {
    let mut h = Harness::new(1).await?;
    h.vault = h
        .vault
        .clone()
        .update_membership(UserMembership::Outsider(UserDataOutsider {
            user_data: h.target.user(),
            status: UserDataOutsiderStatus::Pending,
        }));
    h.save_vault().await?;
    let request = JoinClusterEvent {
        candidate: h.target.user(),
        request_id: REQUEST.into(),
    };
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0
        .value
        .requests
        .insert(VaultActionRequestEvent::JoinCluster(request.clone()));
    h.p.repo.replace(log).await?;
    let receipt = Manager::new(&h, web)
        .join(CanonicalDecisionInput {
            request_id: REQUEST.into(),
            accepted: false,
        })
        .await
        .context("RED: exact Join binding unavailable")?;
    assert_eq!(receipt.request_id.as_deref(), Some(REQUEST));
    assert_eq!(serde_json::to_value(&receipt)?["kind"], "join");
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    let decisions: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.0.value {
            VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(update)) => {
                Some(update)
            }
            _ => None,
        })
        .collect();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].request, request);
    safe(&receipt)?;
    Ok(())
}

#[tokio::test]
async fn web_exact_current_join() -> Result<()> {
    exact_current_join(true).await
}
#[tokio::test]
async fn mobile_exact_current_join() -> Result<()> {
    exact_current_join(false).await
}

async fn execute<T>(
    service: &MetaClientService<Repo, Protocol>,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        result = operation => result,
        result = service.run() => { result?; anyhow::bail!("service exited before checked reply") },
        _ = tokio::time::sleep(Duration::from_secs(10)) => anyhow::bail!("checked service watchdog elapsed"),
    }
}

fn safe(receipt: &LiveboardOperationReceipt) -> Result<()> {
    let json = serde_json::to_value(receipt)?;
    let allowed = [
        "kind",
        "secretId",
        "claimId",
        "requestId",
        "phase",
        "errorCode",
    ];
    assert!(
        json.as_object()
            .context("receipt object")?
            .keys()
            .all(|k| allowed.contains(&k.as_str()))
    );
    for marker in [
        VALUE,
        "PRIVATE-ERROR-CONTENT",
        "shareBlocks",
        "secretMessage",
        "signature",
        "transportSk",
    ] {
        assert!(
            !json.to_string().contains(marker),
            "receipt must contain only operation metadata"
        );
    }
    Ok(())
}

async fn actual_remapped_create(web: bool) -> Result<()> {
    let mut h = Harness::new(1).await?;
    let original = MetaPasswordId::build_from_str("binding-recreated-name");
    h.vault.deleted_secrets.insert(original.clone());
    h.save_vault().await?;
    let receipt = Manager::new(&h, web)
        .create(PlainPassInfo {
            pass_id: original.clone(),
            pass: VALUE.into(),
        })
        .await
        .context("RED: checked binding must return the real persisted Create receipt")?;
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    let added: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(add)) => {
                Some(&add.meta_pass_id)
            }
            _ => None,
        })
        .collect();
    assert_eq!(added.len(), 1);
    assert_ne!(added[0].id, original.id);
    assert_eq!(
        receipt.secret_id.as_deref(),
        Some(added[0].id_str().as_str())
    );
    assert_eq!(serde_json::to_value(&receipt)?["kind"], "create");
    safe(&receipt)?;
    Ok(())
}

#[tokio::test]
async fn web_actual_remapped_create() -> Result<()> {
    actual_remapped_create(true).await
}
#[tokio::test]
async fn mobile_actual_remapped_create() -> Result<()> {
    actual_remapped_create(false).await
}

async fn same_actual_recovery(web: bool) -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("binding-recover").await?;
    let manager = Manager::new(&h, web);
    // Repeated API calls share one live owner, as in the application. Dropping
    // a run closes its transfer and is deliberately not a service restart.
    execute(manager.service(), async {
        let first = manager
            .recover(pass.clone())
            .await
            .context("RED: checked Recovery binding unavailable")?;
        let second = manager.recover(pass.clone()).await?;
        assert_eq!(first.secret_id.as_deref(), Some(pass.id_str().as_str()));
        assert!(first.claim_id.is_some());
        assert_eq!(first.claim_id, second.claim_id);
        assert_eq!(serde_json::to_value(&first)?["kind"], "recover");
        safe(&first)?;
        safe(&second)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn web_same_actual_recovery() -> Result<()> {
    same_actual_recovery(true).await
}
#[tokio::test]
async fn mobile_same_actual_recovery() -> Result<()> {
    same_actual_recovery(false).await
}

async fn verified_exact_remove(web: bool) -> Result<()> {
    let h = Harness::new(3).await?;
    h.protocol.enable_current_guarded();
    h.seed_removal().await?;
    let receipt = Manager::new(&h, web)
        .remove(CanonicalDecisionInput {
            request_id: REQUEST.into(),
            accepted: false,
        })
        .await
        .context("RED: exact Remove binding unavailable")?;
    assert_eq!(receipt.request_id.as_deref(), Some(REQUEST));
    let writes = h.protocol.writes.lock().unwrap();
    assert!(
        !writes.is_empty(),
        "exact decision must reach the ordinary signed flush"
    );
    let first = &writes[0];
    for write in writes.iter() {
        write.verify(&h.approver.device().keys.dsa_pk)?;
        let SignedActionPayload::GuardedRemovalDecision(packet) = &write.payload else {
            anyhow::bail!("exact binding used a legacy target-only removal packet");
        };
        assert_eq!(packet.request_id, REQUEST);
        assert_eq!(
            write.nonce, first.nonce,
            "refresh may retry one command, not create another"
        );
        assert_eq!(write.payload, first.payload);
    }
    safe(&receipt)?;
    Ok(())
}

#[tokio::test]
async fn web_verified_exact_remove() -> Result<()> {
    verified_exact_remove(true).await
}
#[tokio::test]
async fn mobile_verified_exact_remove() -> Result<()> {
    verified_exact_remove(false).await
}

async fn fallible_real_save_error(web: bool) -> Result<()> {
    let h = Harness::new(1).await?;
    let before = h.p.repo.get_db().await;
    h.p.repo.arm(Fault::BeforeDeviceLog);
    let error = Manager::new(&h, web)
        .create(PlainPassInfo::new("binding-fault".into(), VALUE.into()))
        .await
        .expect_err("pre-persistence failure must not report success");
    assert_eq!(
        h.p.repo.hits.load(Ordering::SeqCst),
        1,
        "RED: binding must invoke the real checked service"
    );
    assert!(h.p.repo.get_db().await == before);
    assert!(!error.to_string().contains("PRIVATE-ERROR-CONTENT"));
    assert!(!error.to_string().contains(VALUE));
    Ok(())
}

#[tokio::test]
async fn web_fallible_real_save_error() -> Result<()> {
    fallible_real_save_error(true).await
}
#[tokio::test]
async fn mobile_fallible_real_save_error() -> Result<()> {
    fallible_real_save_error(false).await
}

#[test]
fn wasm_additive_exports_compile_without_changing_legacy_method_signatures() {
    use meta_secret_wasm::wasm_app_manager::WasmApplicationManager as Wasm;
    let _ = Wasm::cluster_distribution_checked;
    let _ = Wasm::recover_checked;
    let _ = Wasm::decide_join_request;
    let _ = Wasm::decide_remove_member;
    let _ = Wasm::cluster_distribution;
    let _ = Wasm::recover_js;
    let _ = Wasm::update_membership;
    let _ = Wasm::approve_remove_member;
    let _ = Wasm::decline_remove_member;
}
