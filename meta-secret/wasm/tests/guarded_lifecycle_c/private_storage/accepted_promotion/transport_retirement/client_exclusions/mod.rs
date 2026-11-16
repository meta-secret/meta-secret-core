//! Actual client exclusions; no simulated domain admission or successful SS response.
mod changed_context;
mod fixture;
mod metadata;
mod mobile_presync_reset;
mod observer;
mod reset_preservation;
mod server_retry;
mod wire;

use super::*;
use async_trait::async_trait;
use meta_secret_core::{
    crypto::{
        key_pair::{KeyPair, TransportDsaKeyPair},
        keys::TransportSk,
        utils::sha256_hex,
    },
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        app::meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        common::model::{
            ApplicationState,
            meta_pass::{PlainPassInfo, SecurePassInfo},
            secret::{SecretDistributionType, SsClaim, SsDistributionId},
            user::common::*,
            vault::{vault::*, vault_data::VaultData},
        },
        db::{
            descriptors::{shared_secret_descriptor::SsLogDescriptor, vault_descriptor::*},
            events::vault::{
                liveboard_metadata::RemovalApplicationState, vault_status::VaultStatusObject,
            },
            objects::persistent_shared_secret::PersistentSharedSecret,
            repo::persistent_credentials::PersistentCredentials,
        },
        security::canonical_json,
    },
    secret::MetaDistributor,
};
use meta_server_node::server::server_app::ServerApp;
use std::collections::HashMap;

fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(value)?)?))
}
fn claim_identity(c: &SsClaim) -> Result<String> {
    digest(&(
        &c.id,
        &c.dist_claim_id,
        &c.vault_name,
        &c.sender,
        &c.distribution_type,
        &c.receivers,
    ))
}
fn result_line(name: &str, phase: &str, result: &Result<()>) {
    eprintln!(
        "CLIENT_EXCLUSIONS variant={name} phase={phase} result_ok={}",
        result.is_ok()
    );
}

struct Baseline {
    origin: GuardedCommandOrigin,
    witness: LocalGuardedPreparationObject,
    index: GenericKvLogEvent,
    registry: GenericKvLogEvent,
    body: DeviceLogObject,
    claims: HashMap<String, String>,
}
impl Baseline {
    async fn capture(p: &fixture::Peer, uuid: &str) -> Result<Self> {
        let witness = p.witness(uuid).await?;
        let origin = witness
            .0
            .value
            .origin
            .clone()
            .context("actual queued origin")?;
        let body = p
            .physical
            .find_one(origin.event_id.clone())
            .await?
            .context("actual own queued N")?
            .device_log()?;
        let index = p
            .physical
            .find_one(ArtifactId::from(LocalGuardedPreparationIndexDescriptor {
                owner: p.actor.user_id(),
            }))
            .await?
            .context("actual index")?;
        let registry = p.registry().await?;
        ensure!(
            witness.0.value.payload.is_some()
                && witness.0.value.transport_noop.is_none()
                && origin.actor_id == *p.actor.device_id()
                && origin.request_id == uuid
                && producer_source_commitment(&body)? == origin.body_commitment,
            "baseline is not actual unsent owned FULL"
        );
        let GenericKvLogEvent::LocalGuardedPreparationIndex(i) = &index else {
            anyhow::bail!("actual index kind")
        };
        ensure!(
            i.0.value.request_ids.iter().filter(|x| *x == uuid).count() == 1,
            "actual witness not enrolled exactly once"
        );
        let GenericKvLogEvent::LocalRemovalEnvelope(r) = &registry else {
            anyhow::bail!("actual registry kind")
        };
        ensure!(
            r.0.value
                .entries
                .iter()
                .any(|e| e.event_id == origin.event_id
                    && e.mode == RemovalEnvelopeMode::Guarded
                    && e.request_id.as_deref() == Some(uuid)),
            "actual owned N lacks exact Guarded registry"
        );
        let mut claims = HashMap::new();
        for event in p.physical.get_db().await.into_values() {
            let values = match event {
                GenericKvLogEvent::SsLog(l) => l.0.value.claims.into_values().collect::<Vec<_>>(),
                GenericKvLogEvent::SsDeviceLog(l) => vec![l.0.value],
                _ => vec![],
            };
            for c in values {
                claims.insert(c.id.wasm_id_str(), claim_identity(&c)?);
            }
        }
        Ok(Self {
            origin,
            witness,
            index,
            registry,
            body,
            claims,
        })
    }
    fn no_material(&self, p: &fixture::Peer) -> Result<()> {
        let mut previous = 0;
        let (mut own_keys, mut saves, mut replacements, mut deletions, mut recovery) =
            (0, 0, 0, 0, 0);
        for call in p.repo.calls.lock().unwrap().iter() {
            ensure!(
                call.order > previous,
                "ALL operation entry trace order regressed"
            );
            previous = call.order;
            match &call.operation {
                observer::Operation::Key(id) => {
                    own_keys += usize::from(id.fqdn == self.origin.event_id.fqdn);
                }
                observer::Operation::Delete(id) | observer::Operation::TryDelete(id) => {
                    deletions += 1;
                    ensure!(
                        *id != self.origin.event_id,
                        "actual owned N deleted by bounded retirement"
                    );
                }
                observer::Operation::Write(replace, _) => {
                    if *replace {
                        replacements += 1;
                    } else {
                        saves += 1;
                    }
                }
                observer::Operation::Recovery => {
                    recovery += 1;
                }
                _ => {}
            }
            let observer::Operation::Write(_, e) = &call.operation else {
                continue;
            };
            let e = e.as_ref();
            let claims = match e {
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_)) => {
                    anyhow::bail!("bounded retry wrote Distribution before first SS import")
                }
                GenericKvLogEvent::SsLog(l) => l.0.value.claims.values().collect::<Vec<_>>(),
                GenericKvLogEvent::SsDeviceLog(l) => vec![&l.0.value],
                GenericKvLogEvent::DeviceLog(b) if b.obj_id().fqdn == self.origin.event_id.fqdn => {
                    ensure!(
                        b.obj_id() == self.origin.event_id,
                        "automatic new own command"
                    );
                    vec![]
                }
                _ => vec![],
            };
            for c in claims {
                if (c.sender == *p.actor.device_id()
                    && c.distribution_type == SecretDistributionType::Split)
                    || self
                        .witness
                        .0
                        .value
                        .claims
                        .iter()
                        .any(|x| x.claim.id == c.id)
                {
                    ensure!(
                        self.claims.get(&c.id.wasm_id_str()) == Some(&claim_identity(c)?),
                        "bounded retry introduced a new owned generation/claim"
                    );
                }
            }
        }
        eprintln!(
            "CLIENT_EXCLUSIONS ALL_operation_audit own_keys={own_keys} saves={saves} replacements={replacements} deletions={deletions} recovery_accessors={recovery} no_Distribution=true no_new_owned_generation=true"
        );
        Ok(())
    }
    async fn binding(&self, p: &fixture::Peer, origin_required: bool) -> Result<()> {
        ensure!(
            p.registry().await? == self.registry
                && p.physical.find_one(self.index.obj_id()).await? == Some(self.index.clone()),
            "retry changed actual index or Guarded registry"
        );
        let deleted = p
            .wire
            .selected
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.1.clone());
        let mut redacted = self.body.clone();
        let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) =
            &mut redacted.0.value
        else {
            anyhow::bail!("actual queued N was not Remove update")
        };
        update
            .request
            .share_transfers
            .retain(|t| deleted.as_ref() != Some(&t.pass_id));
        update
            .share_transfers
            .retain(|t| deleted.as_ref() != Some(&t.pass_id));
        ensure!(
            p.physical.find_one(self.origin.event_id.clone()).await? == Some(redacted.to_generic()),
            "actual retained N differs from exact original with applicable BOTH-level transfer redaction"
        );
        if origin_required {
            let w = p.witness(&self.origin.request_id).await?;
            ensure!(
                w.0.value.origin.as_ref() == Some(&self.origin)
                    && w.0.value.previous_attempts == self.witness.0.value.previous_attempts,
                "retry rewrote immutable origin/history"
            );
        }
        self.no_material(p)
    }
    fn authenticated_cleanup(&self, p: &fixture::Peer, auth: &DeviceTransportNoop) -> Result<()> {
        let mut scrubbed = false;
        for call in p.repo.calls.lock().unwrap().iter() {
            let observer::Operation::Write(replace, e) = &call.operation else {
                continue;
            };
            let e = e.as_ref();
            let private_scrub = matches!(e, GenericKvLogEvent::LocalGuardedPreparation(w)
                if w.obj_id() == self.witness.obj_id() && w.0.value.payload.is_none());
            let owned_change =
                e.obj_id() == self.origin.event_id && *e != self.body.clone().to_generic();
            if private_scrub || owned_change {
                ensure!(*replace && p.repo.reads.lock().unwrap().iter().any(|r| r.order < call.order
                    && matches!(&r.event, Some(GenericKvLogEvent::LocalGuardedPreparation(w))
                        if w.obj_id() == self.witness.obj_id()
                        && w.0.value.transport_noop.as_ref().is_some_and(|n| n.authorization == *auth))),
                    "actual private/owned N scrub occurred before durable authorization OUTER readback");
            }
            if let GenericKvLogEvent::LocalGuardedPreparation(w) = e
                && w.obj_id() == self.witness.obj_id()
            {
                ensure!(
                    !scrubbed || w.0.value.payload.is_none(),
                    "FULL resurrected after scrub"
                );
                scrubbed |= w.0.value.payload.is_none();
            }
        }
        Ok(())
    }
}
