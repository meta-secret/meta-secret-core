//! Real independent clients; the observer delegates every request to ServerApp.
mod future_local;
mod recovery_remote;
mod setup;

use super::{recovery_inflight_fixture::local_share, *};
use crate::server::server_sync_protocol::fixture::{EmbeddedSyncProtocol, SyncProtocolFixture};
use meta_secret_core::node::{
    api::{CanonicalDecisionInput, SignedActionPayload},
    app::{
        meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::{
        ApplicationState, IdString, UserMemberFullInfo, VaultFullInfo,
        liveboard::LiveboardOperationKind,
        liveboard_producer::{
            LiveboardProducerReceipt, ProducerDecisionStage, ProducerKind, ProducerSourceRef,
            producer_source_commitment,
        },
        meta_pass::{MetaPasswordId, PlainPassInfo},
        secret::{SsClaim, SsDistributionStatus},
    },
    db::{
        events::{
            generic_log_event::GenericKvLogEvent,
            shared_secret_event::{SsDeviceLogObject, SsWorkflowObject},
            vault::{
                liveboard_metadata::{DecisionOutcome, RemovalApplicationState},
                producer_metadata::{VerifiedProducerKind, VerifiedProducerSubject},
            },
        },
        objects::{
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_shared_secret::PersistentSharedSecret,
        },
        repo::{generic_db::FindOneQuery, persistent_credentials::PersistentCredentials},
    },
};
use std::sync::Mutex;

const VALUE: &str = "same-sender-synthetic-core-regression";
type Client = MetaClientService<InMemKvLogEventRepo, ObservedWire>;

pub(super) struct ObservedWire {
    inner: Arc<EmbeddedSyncProtocol>,
    actions: Mutex<Vec<SignedAction>>,
}
impl SyncProtocol for ObservedWire {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        if let SyncRequest::Write(write) = &request
            && let WriteSyncRequest::Event(action) = write.as_ref()
        {
            self.actions.lock().unwrap().push(action.clone());
        }
        Box::pin(self.inner.send(request)).await
    }
}
impl ObservedWire {
    pub fn actions(&self) -> Vec<SignedAction> {
        self.actions.lock().unwrap().clone()
    }
    pub fn assert_no_recovery_decisions(&self) {
        assert!(
            !self.actions().iter().any(|action| matches!(
                &action.payload,
                SignedActionPayload::Event(GenericKvLogEvent::SsWorkflow(
                    SsWorkflowObject::Recovery(_) | SsWorkflowObject::Decline(_)
                ))
            )),
            "no participant may answer Recover to manufacture membership retirement"
        );
    }
}

pub(super) struct Actor {
    pub creds: UserCreds,
    pub service: Client,
}
impl Actor {
    fn new(
        p: Arc<PersistentObject<InMemKvLogEventRepo>>,
        creds: UserCreds,
        master: TransportSk,
        wire: Arc<ObservedWire>,
    ) -> Self {
        let gateway = Arc::new(SyncGateway::new(
            "same-sender-real-client".into(),
            p.clone(),
            wire,
            master.clone(),
        ));
        Self {
            service: MetaClientService {
                data_transfer: Arc::new(MetaClientDataTransfer {
                    dt: MetaClientRequestTransfer::new(),
                }),
                sync_gateway: gateway,
                state_provider: Arc::new(MetaClientStateProvider::new()),
                p_obj: p,
                device_data: creds.device(),
                master_key: master,
            },
            creds,
        }
    }
    pub async fn member(&self) -> Result<UserMemberFullInfo> {
        match self.service.get_app_state().await? {
            ApplicationState::Vault(VaultFullInfo::Member(info)) => Ok(*info),
            _ => anyhow::bail!("expected actual current Member state"),
        }
    }
}

pub(super) struct Scenario {
    pub a: Actor,
    pub b: Actor,
    pub c: Actor,
    pub pass: MetaPasswordId,
    pub wire: Arc<ObservedWire>,
    canonical: Arc<PersistentObject<InMemKvLogEventRepo>>,
}
impl Scenario {
    async fn assert_full_copy(&self, actor: &Actor) -> Result<()> {
        let share = local_share(&actor.service.p_obj, &self.pass, &actor.creds).await?;
        assert_eq!(share.share_blocks[0].config.threshold, 1);
        assert!(
            meta_secret_core::recover_from_shares(vec![share])?.text == VALUE,
            "actual own full copy must recover synthetic value"
        );
        Ok(())
    }
    // Host-only verified correlation; private records never supply client authority.
    async fn assert_local_source(
        &self,
        label: &str,
        source: &ProducerSourceRef,
        kind: VerifiedProducerKind,
        id: &str,
    ) -> Result<DeviceLogObject> {
        let body = self
            .a
            .service
            .p_obj
            .repo
            .find_one(source.event_id.clone())
            .await?
            .context(format!("{label}: actual local body"))?
            .device_log()?;
        assert!(
            body.obj_id() == source.event_id,
            "{label}: exact complete source ID"
        );
        assert_eq!(
            producer_source_commitment(&body)?,
            source.body_commitment,
            "{label}: actual body commitment"
        );
        let raw = body.clone().to_generic();
        assert!(
            self.canonical
                .repo
                .find_one(source.event_id.clone())
                .await?
                == Some(raw.clone()),
            "{label}: exact server raw body"
        );
        let actions = self.wire.actions();
        let action = actions
            .iter()
            .find(|action| {
                matches!(&action.payload,
                    SignedActionPayload::Event(event) if *event == raw
                )
            })
            .context(format!("{label}: actually forwarded ordinary signed Event"))?;
        assert_eq!(
            action.signer,
            *self.a.creds.device_id(),
            "{label}: actual signer A"
        );
        action
            .verify(&self.a.creds.device().keys.dsa_pk)
            .context(format!("{label}: actual A signature"))?;
        assert_eq!(
            action.stream,
            source.event_id.fqdn.clone().id_str(),
            "{label}: full-ID stream"
        );
        assert_eq!(
            action.nonce, source.event_id.id.curr as u64,
            "{label}: full-ID nonce"
        );
        let log = PersistentVault::from(self.canonical.clone())
            .get_vault_log_artifact(self.a.creds.vault_name.clone())
            .await?;
        let record = log
            .0
            .value
            .producer_source(&source.event_id)
            .context(format!("{label}: unique private producer record"))?
            .context(format!("{label}: actual private producer record"))?;
        assert_eq!(record.kind, kind, "{label}: verified producer kind");
        assert!(
            record.source_event_id == source.event_id,
            "{label}: verified complete source ID"
        );
        assert_eq!(
            record.body_commitment, source.body_commitment,
            "{label}: verified body commitment"
        );
        assert_eq!(record.request_id, id, "{label}: same next UUID");
        assert_eq!(
            record.actor_id,
            *self.a.creds.device_id(),
            "{label}: verified actor A"
        );
        assert!(
            record.vault_name == self.a.creds.vault_name,
            "{label}: exact Vault"
        );
        assert!(
            record.subject == VerifiedProducerSubject::Device(self.b.creds.device_id().clone()),
            "{label}: target B subject"
        );
        let digest = meta_secret_core::crypto::utils::sha256_hex(&String::from_utf8(
            canonical_json(action)?,
        )?);
        assert_eq!(
            record.verified_envelope_digest, digest,
            "{label}: verified full signed-envelope digest"
        );
        Ok(body)
    }
}
