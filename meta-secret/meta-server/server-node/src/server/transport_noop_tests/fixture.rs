use super::repo::Repo;
use crate::server::{
    server_app::ServerApp, server_data_sync::ServerSyncGateway,
    state_invalidation::NoopStateInvalidationPublisher,
};
use anyhow::{Context, Result};
use meta_secret_core::{
    crypto::utils::sha256_hex,
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        api::*,
        common::model::{
            IdString,
            user::{common::*, user_creds::UserCreds},
            vault::vault::*,
        },
        db::{
            descriptors::{
                local_removal_envelope::LocalRemovalEnvelopeDescriptor,
                shared_secret_descriptor::SsLogDescriptor, vault_descriptor::*,
            },
            events::{
                generic_log_event::*,
                kv_log_event::*,
                local_removal_envelope::*,
                object_id::*,
                shared_secret_event::SsLogObject,
                vault::{
                    device_log_event::DeviceLogObject, vault_event::VaultObject,
                    vault_log_event::*, vault_status::VaultStatusObject,
                },
            },
            objects::{
                persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
                persistent_vault::VaultTail,
            },
            repo::generic_db::*,
        },
        security::{canonical_json, sign_event},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
pub const PURPOSE: &str = "meta-secret.device-transport-noop.v1";
pub struct H {
    pub a: UserCreds,
    pub b: UserCreds,
    pub c: UserCreds,
    pub outsider: UserCreds,
    pub p: Arc<PersistentObject<Repo>>,
    pub server: ServerApp<Repo>,
}
impl H {
    pub async fn new() -> Result<Self> {
        let f = FixtureRegistry::empty().state;
        let a = f.user_creds.client;
        let b = f.user_creds.client_b;
        let c = f.user_creds.vd;
        let mut outsider = FixtureRegistry::empty().state.user_creds.client;
        outsider.vault_name = a.vault_name.clone();
        let p = Arc::new(PersistentObject::new(Arc::new(Repo::default())));
        let mut vault = VaultObject::sign_up(a.vault_name.clone(), a.user().into());
        for actor in [&b, &c] {
            vault.0.value = vault
                .0
                .value
                .update_membership(UserMembership::Member(actor.user().into()));
        }
        vault.0.value = vault.0.value.update_membership(UserMembership::Outsider(
            UserDataOutsider::non_member(outsider.user()),
        ));
        p.repo.save(vault).await?;
        p.repo.save(VaultLogObject::create(a.user().into())).await?;
        for actor in [&a, &b, &c] {
            p.repo
                .save(VaultStatusObject::new(
                    VaultStatus::Member(actor.user().into()),
                    ArtifactId::from(VaultStatusDescriptor::from(actor.user_id())),
                ))
                .await?;
        }
        p.repo
            .save(VaultStatusObject::new(
                VaultStatus::Outsider(UserDataOutsider::non_member(outsider.user())),
                ArtifactId::from(VaultStatusDescriptor::from(outsider.user_id())),
            ))
            .await?;
        p.repo
            .save(SsLogObject(KvLogEvent {
                key: KvKey::from(SsLogDescriptor::from(a.vault_name.clone())),
                value: Default::default(),
            }))
            .await?;
        let server = ServerApp::new(p.repo.clone(), f.device_creds.server_master_key)?;
        server.init().await?;
        Ok(Self {
            a,
            b,
            c,
            outsider,
            p,
            server,
        })
    }
    pub fn gateway(&self) -> ServerSyncGateway<Repo> {
        ServerSyncGateway::new(self.p.clone(), Arc::new(NoopStateInvalidationPublisher))
    }
    pub async fn next(&self, actor: &UserCreds) -> Result<ArtifactId> {
        self.p
            .find_free_id_by_obj_desc(DeviceLogDescriptor::from(actor.user_id()))
            .await
    }
    pub async fn authorization(&self, actor: &UserCreds) -> Result<Value> {
        let id = self.next(actor).await?;
        let value = json!({"version":1,"purpose":PURPOSE,"vaultName":actor.vault_name,"actorId":actor.device_id(),"eventId":id,"stream":id.fqdn.clone().id_str(),"nonce":id.id.curr as u64,"clientEntryCommitment":sha256_hex("opaque client correlation only; no asserted prior business")});
        sign(actor, value)
    }
    pub async fn submit(&self, authorization: Value) -> Result<DataSyncResponse> {
        let request = decode(authorization)?;
        self.server.handle_client_request(request).await
    }
    pub fn tail(&self, actor: &UserCreds) -> VaultTail {
        VaultTail {
            vault_status: ArtifactId::from(VaultStatusDescriptor::from(actor.user_id())),
            vault_log: ArtifactId::from(VaultLogDescriptor::from(actor.vault_name.clone())),
            vault: ArtifactId::from(VaultDescriptor::from(actor.vault_name.clone())),
        }
    }
    pub async fn private(&self) -> Result<LocalRemovalEnvelopeObject> {
        let record = LocalRemovalEnvelopeObject(KvLogEvent {
            key: KvKey::from(LocalRemovalEnvelopeDescriptor(self.a.user_id())),
            value: LocalRemovalEnvelopeRegistry {
                owner: self.a.user_id(),
                first_managed_removal_nonce: 1,
                entries: vec![],
                producer_context: None,
            },
        });
        self.p.repo.save(record.clone()).await?;
        Ok(record)
    }
    pub async fn business(
        &self,
        actor: &UserCreds,
        action: VaultActionEvent,
    ) -> Result<DeviceLogObject> {
        let mut key = KvKey::from(DeviceLogDescriptor::from(actor.user_id()));
        key.obj_id = self.next(actor).await?;
        let body = DeviceLogObject(KvLogEvent { key, value: action });
        let signed = sign_event(
            body.clone().to_generic(),
            actor.device_id().clone(),
            &actor.device_creds.key_manager()?.dsa,
        )?;
        self.server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed,
            ))))
            .await?
            .ensure_success()?;
        Ok(body)
    }
    pub fn removal(&self) -> RemoveMemberRequestEvent {
        RemoveMemberRequestEvent {
            sender: self.a.user().into(),
            target: self.b.user().into(),
            share_transfers: vec![],
        }
    }
    pub async fn next_tail(&self, actor: &UserCreds) -> Result<Option<ArtifactId>> {
        Ok(self
            .server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
                ServerTailRequest {
                    sender: actor.user(),
                    producer_reference: None,
                },
            ))))
            .await?
            .to_server_tail()?
            .device_log_tail)
    }
    pub async fn typed_tail(&self, actor: &UserCreds) -> Result<Option<ArtifactId>> {
        PersistentDeviceLog::from(self.p.clone())
            .find_tail_id(&actor.user_id())
            .await
    }
}
pub fn sign(actor: &UserCreds, mut authorization: Value) -> Result<Value> {
    authorization
        .as_object_mut()
        .context("flat object")?
        .remove("signature");
    let unsigned = String::from_utf8(canonical_json(&authorization)?)?;
    let signature = actor.device_creds.key_manager()?.dsa.sign(unsigned);
    authorization["signature"] = serde_json::to_value(signature)?;
    Ok(authorization)
}
pub fn decode(authorization: Value) -> Result<SyncRequest> {
    serde_json::from_value(json!({"write":{"deviceTransportNoop":authorization}})).context(
        "RED wire gate: actual SyncRequest decoder must accept the approved distinct signed no-op",
    )
}
pub fn stable(response: &DataSyncResponse, authorization: &Value) -> Result<Value> {
    let wire = serde_json::to_value(response)?;
    let body = if wire.get("transportState").is_some() {
        wire
    } else {
        wire.as_object()
            .context("dedicated receipt wrapper")?
            .values()
            .find(|v| v.get("transportState").is_some())
            .context("typed public receipt")?
            .clone()
    };
    let mut stable = body.as_object().context("public receipt object")?.clone();
    stable.remove("currentNextTail");
    let allow = [
        "version",
        "eventId",
        "actorId",
        "vaultName",
        "clientEntryCommitment",
        "authorizationHash",
        "transportState",
        "recordEffect",
    ];
    assert_eq!(stable.len(), allow.len());
    assert!(stable.keys().all(|key| allow.contains(&key.as_str())));
    for key in [
        "version",
        "eventId",
        "actorId",
        "vaultName",
        "clientEntryCommitment",
    ] {
        assert_eq!(stable[key], authorization[key]);
    }
    assert_eq!(stable["transportState"], "noop_committed");
    assert_eq!(stable["recordEffect"], "not_applied");
    let expected = sha256_hex(&String::from_utf8(canonical_json(authorization)?)?);
    assert_eq!(stable["authorizationHash"], expected);
    for hidden in [
        "signature",
        "shareTransfers",
        "requestId",
        "secretMessage",
        "cipherText",
        "transportSk",
    ] {
        assert!(!serde_json::to_string(&stable)?.contains(hidden));
    }
    Ok(Value::Object(stable))
}
