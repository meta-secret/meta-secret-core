//! Contract tests use real repositories, signatures and the public ordered server boundary.
//! The wire fixtures allow RED to compile before the additive protocol types exist.
mod guarded_security;
mod integration_binding_delete;
mod integration_binding_fixture;
mod integration_binding_observation;
mod integration_binding_remove;
mod persistence_faults;
mod prepared_faults;
mod recovery;
mod recovery_authorization;
mod recovery_inflight_backends;
mod recovery_inflight_controls;
mod recovery_inflight_durability;
mod recovery_inflight_fixture;
mod recovery_inflight_responses;
mod recovery_inflight_shapes;
mod recovery_intake_backends;
mod recovery_intake_containment;
mod recovery_intake_durability;
mod recovery_intake_faults;
mod recovery_intake_fixture;
mod recovery_intake_inflight;
mod recovery_intake_ordering;
mod recovery_partial;
mod recovery_receipt_backends;
mod recovery_receipt_corrupt;
mod recovery_receipt_durability;
mod recovery_receipt_faults;
mod recovery_receipt_fixture;
mod recovery_receipt_ordering;
mod recovery_receipt_privacy;
mod recovery_receipt_storage;
mod removal;
mod same_sender_pending_recovery_fixture;
mod signed_compatibility;

mod correlation_automatic;
mod correlation_backends;
mod correlation_faults;
mod correlation_privacy;
mod correlation_provenance;
mod correlation_purge;
mod correlation_support;
mod producer_guard_cleanup;
mod producer_guard_direct;
mod producer_guard_fixture;
mod producer_guard_membership;
mod split_delivery_regression;
mod transport_noop_b;

use super::server_app::ServerApp;
use anyhow::{Context, Result};
use meta_secret_core::crypto::encoding::base64::Base64Text;
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
use meta_secret_core::node::api::{
    DataSyncResponse, ReadSyncRequest, SignedAction, SyncRequest, VaultRequest, WriteSyncRequest,
};
use meta_secret_core::node::common::model::user::common::{
    UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use meta_secret_core::node::common::model::user::user_creds::UserCreds;
use meta_secret_core::node::db::descriptors::vault_descriptor::{
    DeviceLogDescriptor, VaultDescriptor, VaultLogDescriptor, VaultStatusDescriptor,
};
use meta_secret_core::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use meta_secret_core::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use meta_secret_core::node::db::events::object_id::{ArtifactId, Next};
use meta_secret_core::node::db::events::vault::device_log_event::DeviceLogObject;
use meta_secret_core::node::db::events::vault::vault_event::VaultObject;
use meta_secret_core::node::db::events::vault::vault_log_event::{
    RemoveMemberDecision, RemoveMemberEvent, RemoveMemberRequestEvent, VaultActionEvent,
    VaultActionRequestEvent, VaultActionUpdateEvent, VaultLogObject,
};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::objects::persistent_vault::{PersistentVault, VaultTail};
use meta_secret_core::node::db::repo::generic_db::{KvLogEventRepo, SaveCommand};
use meta_secret_core::node::security::{canonical_json, sign_event};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

struct Harness {
    a: UserCreds,
    b: UserCreds,
    c: UserCreds,
    master_key: TransportSk,
    p_obj: Arc<PersistentObject<InMemKvLogEventRepo>>,
    server: ServerApp<InMemKvLogEventRepo>,
}

impl Harness {
    async fn new() -> Result<Self> {
        let fixture = FixtureRegistry::empty().state;
        let a = fixture.user_creds.client;
        let b = fixture.user_creds.client_b;
        let c = fixture.user_creds.vd;
        let p_obj = fixture.p_obj.server;
        let mut vault = VaultObject::sign_up(a.vault_name.clone(), member(&a));
        vault.0.value = vault
            .0
            .value
            .update_membership(UserMembership::Member(member(&b)))
            .update_membership(UserMembership::Member(member(&c)));
        p_obj.repo.save(vault).await?;
        p_obj.repo.save(VaultLogObject::create(member(&a))).await?;
        let master_key = fixture.device_creds.server_master_key;
        let server = ServerApp::new(p_obj.repo.clone(), master_key.clone())?;
        server.init().await?;
        Ok(Self {
            a,
            b,
            c,
            master_key,
            p_obj,
            server,
        })
    }

    fn removal(&self) -> RemoveMemberRequestEvent {
        RemoveMemberRequestEvent {
            sender: member(&self.a),
            target: member(&self.b),
            share_transfers: vec![],
        }
    }

    fn decision(&self, accepted: bool) -> RemoveMemberEvent {
        let request = self.removal();
        let new_status = if accepted {
            UserMembership::Outsider(UserDataOutsider {
                user_data: self.b.user(),
                status: UserDataOutsiderStatus::Declined,
            })
        } else {
            UserMembership::Member(member(&self.b))
        };
        RemoveMemberEvent {
            request,
            sender: member(&self.c),
            target: member(&self.b),
            new_status,
            decision: if accepted {
                RemoveMemberDecision::Accepted
            } else {
                RemoveMemberDecision::Declined
            },
            share_transfers: vec![],
        }
    }

    async fn send_action(&self, actor: &UserCreds, event: VaultActionEvent) -> Result<()> {
        let event = next_event(&self.p_obj, actor, event).await?;
        let signed = sign_event(
            event.to_generic(),
            actor.device_id().clone(),
            &actor.device_creds.key_manager()?.dsa,
        )?;
        send(&self.server, signed).await
    }

    async fn request(&self) -> Result<()> {
        self.send_action(
            &self.a,
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(self.removal())),
        )
        .await
    }

    async fn decide(&self, accepted: bool) -> Result<()> {
        self.send_action(
            &self.c,
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(
                self.decision(accepted),
            )),
        )
        .await
    }

    async fn historical_pending(&self) -> Result<()> {
        let mut log = VaultLogObject::create(member(&self.a));
        log.0.key.obj_id = log.obj_id().next();
        log.0
            .value
            .requests
            .insert(VaultActionRequestEvent::RemoveMember(self.removal()));
        self.p_obj.repo.save(log).await?;
        Ok(())
    }

    async fn refresh(&self) -> Result<()> {
        refresh(&self.server, &self.a).await
    }

    async fn log(&self) -> Result<Value> {
        let log = PersistentVault::from(self.p_obj.clone())
            .get_vault_log_artifact(self.a.vault_name.clone())
            .await?;
        Ok(serde_json::to_value(log.0.value)?)
    }

    async fn pending_id(&self) -> Result<String> {
        let log = self.log().await?;
        let pending = log
            .get("liveboardRemovalPending")
            .and_then(Value::as_array)
            .context("RED: canonical pending removal has no server-owned occurrence binding")?;
        anyhow::ensure!(
            pending.len() == 1,
            "expected exactly one current removal occurrence"
        );
        let id = pending[0]
            .get("requestId")
            .and_then(Value::as_str)
            .context("binding UUID is missing")?;
        anyhow::ensure!(
            id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4,
            "binding must be an opaque UUID"
        );
        Ok(id.to_owned())
    }

    async fn reopen(&self) -> Result<ServerApp<InMemKvLogEventRepo>> {
        ServerApp::new(self.p_obj.repo.clone(), self.master_key.clone())
    }
}

fn member(creds: &UserCreds) -> UserDataMember {
    UserDataMember::from(creds.user())
}

async fn next_event<R: KvLogEventRepo>(
    p_obj: &Arc<PersistentObject<R>>,
    actor: &UserCreds,
    action: VaultActionEvent,
) -> Result<DeviceLogObject> {
    let desc = DeviceLogDescriptor::from(actor.user_id());
    let mut key = KvKey::from(desc.clone());
    key.obj_id = p_obj.find_free_id_by_obj_desc(desc).await?;
    Ok(DeviceLogObject(KvLogEvent { key, value: action }))
}

async fn send<R: KvLogEventRepo>(server: &ServerApp<R>, action: SignedAction) -> Result<()> {
    server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            action,
        ))))
        .await?
        .ensure_success()
}

async fn refresh<R: KvLogEventRepo>(server: &ServerApp<R>, actor: &UserCreds) -> Result<()> {
    let name = actor.vault_name.clone();
    let tail = VaultTail {
        vault_log: ArtifactId::from(VaultLogDescriptor::from(name.clone())),
        vault: ArtifactId::from(VaultDescriptor::from(name)),
        vault_status: ArtifactId::from(VaultStatusDescriptor::from(actor.user_id())),
    };
    let response = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: actor.user(),
                tail,
            },
        ))))
        .await?;
    anyhow::ensure!(
        matches!(response, DataSyncResponse::Data(_)),
        "canonical refresh must return Data"
    );
    Ok(())
}

#[derive(Serialize)]
struct WireUnsigned<'a> {
    signer: &'a meta_secret_core::node::common::model::device::common::DeviceId,
    nonce: u64,
    stream: &'a str,
    payload: &'a Value,
}

fn signed_wire(actor: &UserCreds, event: &DeviceLogObject, payload: Value) -> Result<Value> {
    let id = event.obj_id();
    use meta_secret_core::node::common::model::IdString;
    let stream = id.fqdn.id_str();
    let unsigned = WireUnsigned {
        signer: actor.device_id(),
        nonce: id.id.curr as u64,
        stream: &stream,
        payload: &payload,
    };
    let signature = actor
        .device_creds
        .key_manager()?
        .dsa
        .sign(String::from_utf8(canonical_json(&unsigned)?)?);
    Ok(
        json!({"signer": actor.device_id(), "nonce": unsigned.nonce, "stream": stream, "signature": signature, "payload": payload}),
    )
}

fn guarded(
    actor: &UserCreds,
    event: &DeviceLogObject,
    request_id: &str,
    version: u64,
) -> Result<SignedAction> {
    let wire = signed_wire(
        actor,
        event,
        json!({"guardedRemovalDecision": {"version": version, "requestId": request_id, "event": event}}),
    )?;
    serde_json::from_value(wire)
        .context("RED: Core does not decode the versioned guarded removal decision")
}

fn result_record<'a>(log: &'a Value, request_id: &str) -> Result<&'a Value> {
    log.get("liveboardRemovalResults")
        .and_then(Value::as_array)
        .and_then(|results| {
            results
                .iter()
                .find(|record| record["requestId"].as_str() == Some(request_id))
        })
        .context("RED: the exact terminal removal result was lost from canonical VaultLog")
}
