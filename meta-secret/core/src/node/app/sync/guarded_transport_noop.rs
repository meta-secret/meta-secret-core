//! Exact technical attempts are independent of Business ACKs and current outbox selection.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    api::{DataSyncResponse, ServerTailResponse, SyncRequest, WriteSyncRequest},
    common::model::{IdString, user::common::UserData},
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{local_guarded_preparation::*, object_id::ArtifactId},
        objects::{
            local_guarded_preparation::{PersistentGuardedPreparation, UNAVAILABLE},
            local_removal_envelope::PersistentRemovalEnvelope,
        },
        repo::generic_db::KvLogEventRepo,
    },
    device_transport_noop::{
        DeviceTransportNoopReceipt, TransportNoopEffect, TransportNoopState, next_transport_id,
        validate_transport_id,
    },
};
use anyhow::{Result, ensure};
use std::collections::HashSet;

pub(super) fn validate_technical(
    user: &UserData,
    origin: &GuardedCommandOrigin,
    technical: &LocalGuardedTransportNoop,
) -> Result<()> {
    let auth = &technical.authorization;
    auth.verify(&user.device.keys.dsa_pk)?;
    ensure!(
        auth.event_id == origin.event_id
            && auth.actor_id == origin.actor_id
            && auth.actor_id == user.device.device_id
            && auth.vault_name == origin.vault_name
            && auth.vault_name == user.vault_name()
            && auth.stream == origin.event_id.fqdn.clone().id_str()
            && auth.nonce == origin.event_id.id.curr as u64
            && auth.client_entry_commitment == origin.body_commitment,
        UNAVAILABLE
    );
    if let Some(receipt) = &technical.receipt {
        validate_receipt(receipt, technical)?;
    }
    if let Some(tail) = &technical.confirmed_next_tail {
        ensure!(technical.receipt.is_some(), UNAVAILABLE);
        advanced(tail, &origin.event_id)?;
    }
    Ok(())
}

fn validate_receipt(
    receipt: &DeviceTransportNoopReceipt,
    technical: &LocalGuardedTransportNoop,
) -> Result<()> {
    let auth = &technical.authorization;
    ensure!(
        receipt.version == 1
            && receipt.event_id == auth.event_id
            && receipt.actor_id == auth.actor_id
            && receipt.vault_name == auth.vault_name
            && receipt.client_entry_commitment == auth.client_entry_commitment
            && receipt.authorization_hash == auth.authorization_hash()?
            && receipt.transport_state == TransportNoopState::NoopCommitted
            && receipt.record_effect == TransportNoopEffect::NotApplied,
        UNAVAILABLE
    );
    Ok(())
}

fn advanced(tail: &ArtifactId, id: &ArtifactId) -> Result<()> {
    validate_transport_id(tail)?;
    ensure!(
        tail.fqdn == id.fqdn && tail.id.curr > id.id.curr,
        UNAVAILABLE
    );
    Ok(())
}

struct Attempt {
    request_id: String,
    previous: Option<usize>,
    event_id: ArtifactId,
}
fn transport(
    value: &LocalGuardedPreparation,
    previous: Option<usize>,
) -> Result<(GuardedCommandOrigin, LocalGuardedTransportNoop)> {
    let pair = match previous {
        None => value.origin.as_ref().zip(value.transport_noop.as_ref()),
        Some(index) => value.previous_attempts.get(index).and_then(|attempt| {
            attempt
                .transport_noop
                .as_ref()
                .map(|n| (&attempt.origin, n))
        }),
    };
    pair.map(|(origin, technical)| (origin.clone(), technical.clone()))
        .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))
}
fn replace_transport(
    value: &mut LocalGuardedPreparation,
    previous: Option<usize>,
    technical: LocalGuardedTransportNoop,
) -> Result<()> {
    match previous {
        None => value.transport_noop = Some(technical),
        Some(index) => {
            value
                .previous_attempts
                .get_mut(index)
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
                .transport_noop = Some(technical)
        }
    }
    Ok(())
}

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    async fn guarded_transport_attempts(&self, user: &UserData) -> Result<Vec<Attempt>> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let mut attempts = vec![];
        let mut ids = HashSet::new();
        for object in store.load_all(&user.user_id()).await? {
            let value = &object.0.value;
            for previous in
                std::iter::once(None).chain((0..value.previous_attempts.len()).map(Some))
            {
                let has_transport = match previous {
                    None => value.transport_noop.is_some(),
                    Some(index) => value.previous_attempts[index].transport_noop.is_some(),
                };
                if !has_transport {
                    continue;
                }
                let (origin, technical) = transport(value, previous)?;
                validate_technical(user, &origin, &technical)?;
                ensure!(ids.insert(origin.event_id.clone()), UNAVAILABLE);
                attempts.push(Attempt {
                    request_id: value.request_id.clone(),
                    previous,
                    event_id: origin.event_id,
                });
            }
        }
        attempts.sort_by_key(|attempt| attempt.event_id.id.curr);
        Ok(attempts)
    }

    pub(super) async fn has_guarded_transport_noop(
        &self,
        user: &UserData,
        id: &ArtifactId,
    ) -> Result<bool> {
        Ok(self
            .guarded_transport_attempts(user)
            .await?
            .iter()
            .any(|attempt| attempt.event_id == *id))
    }

    pub(super) async fn sync_guarded_transport_noops(
        &self,
        user: &UserData,
        initial: &ServerTailResponse,
    ) -> Result<ServerTailResponse> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let mut tail = initial.clone();
        for attempt in self.guarded_transport_attempts(user).await? {
            let mut object = store
                .load(&user.user_id(), &attempt.request_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            let (origin, mut technical) = transport(&object.0.value, attempt.previous)?;
            let registry = PersistentRemovalEnvelope {
                p_obj: self.p_obj.clone(),
            }
            .load(&user.user_id())
            .await?;
            let entry = registry.as_ref().and_then(|registry| {
                registry
                    .0
                    .value
                    .entries
                    .iter()
                    .find(|entry| entry.event_id == origin.event_id)
            });
            self.validate_guarded_envelope_binding(user, &origin.event_id, entry)
                .await?;
            let head = tail
                .device_log_tail
                .clone()
                .unwrap_or_else(|| ArtifactId::from(DeviceLogDescriptor::from(user.user_id())));
            ensure!(head.fqdn == origin.event_id.fqdn, UNAVAILABLE);
            if technical.confirmed_next_tail.is_some() {
                advanced(&head, &origin.event_id)?;
                continue;
            }
            if origin.event_id.id.curr > head.id.curr {
                continue; // Do not skip a preceding ordinary command or choose a different nonce.
            }
            let response = self
                .sync
                .send(SyncRequest::Write(Box::new(
                    WriteSyncRequest::DeviceTransportNoop(technical.authorization.clone()),
                )))
                .await?;
            let DataSyncResponse::DeviceTransportNoop(receipt) = response else {
                anyhow::bail!(UNAVAILABLE)
            };
            validate_receipt(&receipt, &technical)?;
            ensure!(
                technical.receipt.as_ref().is_none_or(|old| *old == receipt),
                UNAVAILABLE
            );
            technical.receipt = Some(receipt);
            let mut value = object.0.value.clone();
            replace_transport(&mut value, attempt.previous, technical.clone())?;
            object = store.replace(&object, value).await?;

            // This read is a separate actual response; a stable receipt contains no tail.
            let fresh = self.get_server_tail(user.clone()).await?;
            let observed = fresh
                .device_log_tail
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
            advanced(observed, &origin.event_id)?;
            ensure!(
                observed.id.curr >= next_transport_id(&origin.event_id)?.id.curr,
                UNAVAILABLE
            );
            technical.confirmed_next_tail = Some(observed.clone());
            let mut value = object.0.value.clone();
            replace_transport(&mut value, attempt.previous, technical)?;
            store.replace(&object, value).await?;
            tail = fresh;
        }
        Ok(tail)
    }
}
