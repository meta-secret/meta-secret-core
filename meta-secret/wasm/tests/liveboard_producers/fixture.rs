use super::{protocol::Network, repo::Repo, seed_fixture};
use anyhow::{Context, Result};
use meta_secret_core::{
    crypto::keys::TransportSk,
    node::{
        app::{
            meta_app::{
                client_transfer::MetaClientRequestTransfer,
                meta_client_service::{
                    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
                },
            },
            sync::sync_gateway::SyncGateway,
        },
        common::model::{
            liveboard_producer::*, meta_pass::MetaPasswordId, user::user_creds::UserCreds,
        },
        db::{
            events::{
                generic_log_event::*,
                vault::{device_log_event::DeviceLogObject, vault_log_event::*},
            },
            in_mem_db::InMemKvLogEventRepo,
            objects::persistent_object::PersistentObject,
            repo::generic_db::*,
        },
    },
};
use meta_server_node::server::server_app::ServerApp;
use std::{future::Future, sync::Arc, time::Duration};

pub struct Harness<R: KvLogEventRepo + Send + Sync> {
    pub p: Arc<PersistentObject<Repo<R>>>,
    pub actor: UserCreds,
    pub target: UserCreds,
    pub master: TransportSk,
    pub primary: MetaPasswordId,
    pub secondary: MetaPasswordId,
    pub network: Arc<Network>,
    pub service: Arc<MetaClientService<Repo<R>, Network>>,
    pub web: meta_secret_wasm::app_manager::ApplicationManager<Repo<R>, Network>,
    pub mobile: mobile_common::app_manager::ApplicationManager<Repo<R>, Network>,
}
pub struct ReopenState {
    pub actor: UserCreds,
    pub target: UserCreds,
    pub master: TransportSk,
    pub primary: MetaPasswordId,
    pub secondary: MetaPasswordId,
    pub network: Arc<Network>,
}
impl<R: KvLogEventRepo + Send + Sync> Harness<R> {
    pub async fn new(inner: Arc<R>, count: usize) -> Result<Self> {
        let mut seed = seed_fixture::Harness::new(count).await?;
        let primary = seed.seed_secret("producer-primary").await?;
        let secondary = seed.seed_secret("producer-secondary").await?;
        let server_repo = Arc::new(InMemKvLogEventRepo::default());
        for value in seed.p.repo.get_db().await.into_values() {
            inner.save(value.clone()).await?;
            if !matches!(
                value,
                GenericKvLogEvent::UserCreds(_)
                    | GenericKvLogEvent::DeviceCreds(_)
                    | GenericKvLogEvent::LocalRemovalEnvelope(_)
            ) {
                server_repo.save(value).await?;
            }
        }
        let server = Arc::new(ServerApp::new(server_repo.clone(), seed.master.clone())?);
        server.init().await?;
        let target = if count == 2 {
            seed.initiator
        } else {
            seed.target
        };
        Self::reopen(
            inner,
            ReopenState {
                actor: seed.approver,
                target,
                master: seed.master,
                primary,
                secondary,
                network: Arc::new(Network::new(server, server_repo)),
            },
        )
    }
    pub fn reopen(inner: Arc<R>, state: ReopenState) -> Result<Self> {
        let p = Arc::new(PersistentObject::new(Arc::new(Repo::new(inner))));
        let gateway = Arc::new(SyncGateway::new(
            "actual-producer-boundary".into(),
            p.clone(),
            state.network.clone(),
            state.master.clone(),
        ));
        let service = Arc::new(MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MetaClientRequestTransfer::new(),
            }),
            sync_gateway: gateway.clone(),
            state_provider: Arc::new(MetaClientStateProvider::new()),
            p_obj: p.clone(),
            device_data: state.actor.device(),
            master_key: state.master.clone(),
        });
        let web = meta_secret_wasm::app_manager::ApplicationManager::new(
            state.network.clone(),
            gateway.clone(),
            service.clone(),
            state.master.clone(),
        );
        let mobile = mobile_common::app_manager::ApplicationManager::new(
            state.network.clone(),
            gateway,
            service.clone(),
            state.master.clone(),
        );
        Ok(Self {
            p,
            actor: state.actor,
            target: state.target,
            master: state.master,
            primary: state.primary,
            secondary: state.secondary,
            network: state.network,
            service,
            web,
            mobile,
        })
    }
    pub fn into_reopen(self) -> ReopenState {
        ReopenState {
            actor: self.actor,
            target: self.target,
            master: self.master,
            primary: self.primary,
            secondary: self.secondary,
            network: self.network,
        }
    }
    pub async fn delete(
        &self,
        mobile: bool,
        pass: MetaPasswordId,
    ) -> Result<LiveboardProducerReceipt> {
        if mobile {
            self.mobile.delete_secret_checked(pass).await
        } else {
            self.web.delete_secret_checked(pass).await
        }
    }
    pub async fn remove(&self, mobile: bool) -> Result<LiveboardProducerReceipt> {
        let target = self.target.device_id().to_string();
        if mobile {
            self.mobile.remove_member_checked(target).await
        } else {
            self.web.remove_member_checked(target).await
        }
    }
    pub async fn reconcile(
        &self,
        mobile: bool,
        input: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        if mobile {
            self.mobile.reconcile_producer_receipt(input).await
        } else {
            self.web.reconcile_producer_receipt(input).await
        }
    }
    pub async fn resume(
        &self,
        mobile: bool,
        input: ProducerReference,
    ) -> Result<LiveboardProducerReceipt> {
        if mobile {
            self.mobile.continue_local_remove_checked(input).await
        } else {
            self.web.continue_local_remove_checked(input).await
        }
    }
}
pub async fn drive<R: KvLogEventRepo + Send + Sync, T>(
    h: &Harness<R>,
    scenario: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        result=scenario=>result,
        result=Box::pin(h.service.run())=>{result?;anyhow::bail!("real producer service ended before scenario")},
        _=tokio::time::sleep(Duration::from_secs(10))=>anyhow::bail!("producer watchdog; no boundary inferred from elapsed time"),
    }
}
pub fn reference(receipt: &LiveboardProducerReceipt) -> ProducerReference {
    ProducerReference {
        version: receipt.version,
        kind: receipt.kind.clone(),
        request_id: receipt.request_id.clone(),
        request_stage: receipt.request_stage.source.clone(),
        decision_stage: match &receipt.decision_stage {
            Some(ProducerDecisionStage::Attempted { source, .. }) => Some(source.clone()),
            _ => None,
        },
    }
}
pub fn request_id(source: &DeviceLogObject) -> Result<Option<String>> {
    match &source.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)) => {
            Ok(Some(request.request_id.clone()))
        }
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(_)) => Ok(None),
        _ => anyhow::bail!("actual producer R missing"),
    }
}
pub fn assert_source(receipt: &LiveboardProducerReceipt, source: &DeviceLogObject) -> Result<()> {
    assert_eq!(receipt.request_stage.source.event_id, source.obj_id());
    if let Some(id) = request_id(source)? {
        assert_eq!(receipt.request_id.as_ref(), Some(&id));
    }
    let commitment = &receipt.request_stage.source.body_commitment;
    anyhow::ensure!(
        commitment.len() == 64
            && commitment
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "actual Core commitment shape"
    );
    // Root-selected exact byte contract: ONE LF, then canonical DeviceLogObject
    // including key; no GenericKvLogEvent wrapper and no signature.
    let canonical = String::from_utf8(meta_secret_core::node::security::canonical_json(source)?)?;
    let expected = meta_secret_core::crypto::utils::sha256_hex(&format!(
        "MetaSecret.LiveboardProducer.Source.v1\n{canonical}"
    ));
    assert!(
        commitment == &expected,
        "Core source commitment must match independently selected exact bytes"
    );
    assert!(
        receipt
            .error_code
            .as_deref()
            .is_none_or(|s| s == "OperationUnavailable")
    );
    assert!(!serde_json::to_string(receipt)?.contains("PRIVATE-PRODUCER-PERSISTENCE-ERROR"));
    Ok(())
}
pub fn first_request(sources: &[DeviceLogObject]) -> Result<&DeviceLogObject> {
    sources
        .iter()
        .find(|event| matches!(event.0.value, VaultActionEvent::Request(_)))
        .context("real attempted producer R")
}
