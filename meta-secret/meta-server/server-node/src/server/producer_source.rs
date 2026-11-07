//! Correlation is minted only after ordinary full signed intake succeeds.
use super::{
    server_data_sync::ServerSyncGateway,
    state_invalidation::{StateInvalidation, StateInvalidationScope},
};
use anyhow::{Result, anyhow, ensure};
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        api::SignedAction,
        common::model::{
            liveboard_producer::{ProducerKind, ProducerReference, ProducerSourceRef},
            user::common::{UserDataMember, UserMembership},
            vault::vault_data::VaultData,
        },
        db::{
            actions::vault::automatic_delete_application::AutomaticDeleteAction,
            descriptors::{
                object_descriptor::ToObjectDescriptor, vault_descriptor::DeviceLogDescriptor,
            },
            events::{
                generic_log_event::{GenericKvLogEvent, ObjIdExtractor},
                object_id::ArtifactId,
                vault::{
                    device_log_event::DeviceLogObject, producer_metadata::*, vault_log_event::*,
                },
            },
            objects::persistent_vault::PersistentVault,
            repo::generic_db::KvLogEventRepo,
        },
        security::canonical_json,
    },
};

pub(super) struct Candidate {
    pub source: DeviceLogObject,
    pub actor: UserDataMember,
    pub kind: VerifiedProducerKind,
    pub subject: VerifiedProducerSubject,
    pub body_commitment: String,
    pub envelope_digest: String,
}

impl Candidate {
    pub fn new(action: &SignedAction, source: &DeviceLogObject) -> Result<Option<Self>> {
        let (actor, kind, subject) = match &source.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r)) => (
                &r.sender,
                VerifiedProducerKind::DeleteRequest,
                VerifiedProducerSubject::Secret(r.pass_id.id_str()),
            ),
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) => (
                &r.sender,
                VerifiedProducerKind::RemoveRequest,
                VerifiedProducerSubject::Device(r.target.user_data.device.device_id.clone()),
            ),
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u))
                if u.decision == RemoveMemberDecision::Accepted && u.sender == u.request.sender =>
            {
                (
                    &u.sender,
                    VerifiedProducerKind::LocalRemoveAccept,
                    VerifiedProducerSubject::Device(u.target.user_data.device.device_id.clone()),
                )
            }
            _ => return Ok(None),
        };
        ensure!(
            actor.user_data.device.device_id == action.signer,
            "ProducerMetadata.Unavailable"
        );
        Ok(Some(Self {
            source: source.clone(),
            actor: actor.clone(),
            kind,
            subject,
            body_commitment: producer_body_commitment(source)?,
            envelope_digest: sha256_hex(&String::from_utf8(canonical_json(action)?)?),
        }))
    }

    pub fn trusted_shape(&self) -> Result<bool> {
        let descriptor = DeviceLogDescriptor::from(self.actor.user_data.user_id());
        if self.source.obj_id().fqdn != ArtifactId::from(descriptor.clone()).fqdn
            || self.source.0.key.obj_desc != descriptor.to_obj_desc()
            || self.actor.user_data.vault_name() != self.source.0.value.vault_name()
        {
            return Ok(false);
        }
        let is_delete = self.kind == VerifiedProducerKind::DeleteRequest;
        let request_id = match &self.source.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r)) => {
                Some(r.request_id.clone())
            }
            _ => None,
        };
        let reference = ProducerReference {
            version: 1,
            kind: if is_delete {
                ProducerKind::DeleteSecret
            } else {
                ProducerKind::RemoveDevice
            },
            request_id,
            request_stage: ProducerSourceRef {
                event_id: self.source.obj_id(),
                body_commitment: self.body_commitment.clone(),
            },
            decision_stage: None,
        };
        Ok(ProducerReference::parse_json(&serde_json::to_string(&reference)?).is_ok())
    }

    pub fn record(
        &self,
        id: String,
        mode: Option<ProducerCompletionMode>,
    ) -> VerifiedProducerSourceRef {
        VerifiedProducerSourceRef {
            kind: self.kind.clone(),
            source_event_id: self.source.obj_id(),
            body_commitment: self.body_commitment.clone(),
            actor_id: self.actor.user_data.device.device_id.clone(),
            vault_name: self.source.0.value.vault_name(),
            request_id: id,
            subject: self.subject.clone(),
            verified_envelope_digest: self.envelope_digest.clone(),
            completion_mode: mode,
        }
    }
}

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn try_apply_producer_source(
        &self,
        action: &SignedAction,
        event: &GenericKvLogEvent,
    ) -> Result<bool> {
        self.apply_producer_source_inner(action, event)
            .await
            .map_err(|_| anyhow!("ProducerMetadata.Unavailable"))
    }

    async fn apply_producer_source_inner(
        &self,
        action: &SignedAction,
        event: &GenericKvLogEvent,
    ) -> Result<bool> {
        let GenericKvLogEvent::DeviceLog(source) = event else {
            return Ok(false);
        };
        let name = source.0.value.vault_name();
        let p = PersistentVault::from(self.p_obj.clone());
        let Some(log) = p.vault_log(name.clone()).await? else {
            return Ok(false);
        };
        let known = log.0.value.producer_source(&source.obj_id())?.cloned();
        let candidate = Candidate::new(action, source)?;
        if let Some(known) = known {
            let candidate = candidate.ok_or_else(|| anyhow!("ProducerMetadata.Unavailable"))?;
            let vault = p.get_vault(name.clone()).await?.to_data();
            ensure!(
                candidate.trusted_shape()?
                    && vault.is_member(&action.signer)
                    && known
                        == candidate
                            .record(known.request_id.clone(), known.completion_mode.clone()),
                "ProducerMetadata.Unavailable"
            );
            ensure!(
                canonical_occurrence(&log.0.value, &vault, &known, source)?,
                "ProducerMetadata.Unavailable"
            );
            AutomaticDeleteAction {
                p_obj: self.p_obj.clone(),
            }
            .repair(name)
            .await?;
            self.finish_producer_raw(source.clone(), &known.kind)
                .await?;
            return Ok(true);
        }
        let Some(candidate) = candidate else {
            return Ok(false);
        };
        let vault = p.get_vault(name).await?.to_data();
        if !candidate.trusted_shape()?
            || vault.find_user(&action.signer)
                != Some(&UserMembership::Member(candidate.actor.clone()))
        {
            return Ok(false);
        }
        self.admit_producer_source(candidate, action, log.0.value, vault)
            .await
    }

    pub(super) async fn finish_producer_raw(
        &self,
        source: DeviceLogObject,
        kind: &VerifiedProducerKind,
    ) -> Result<()> {
        let name = source.0.value.vault_name();
        self.p_obj.save_verified(source).await?;
        super::producer_public::repair_raw_cleanup(self, &name).await?;
        let scope = if *kind == VerifiedProducerKind::DeleteRequest {
            StateInvalidationScope::Vault
        } else {
            StateInvalidationScope::Devices
        };
        self.invalidation_publisher
            .publish(StateInvalidation::new(name.clone(), scope));
        if *kind == VerifiedProducerKind::LocalRemoveAccept {
            self.invalidation_publisher.publish(StateInvalidation::new(
                name,
                StateInvalidationScope::SsClaims,
            ));
        }
        Ok(())
    }
}

fn canonical_occurrence(
    events: &VaultActionEvents,
    vault: &VaultData,
    record: &VerifiedProducerSourceRef,
    source: &DeviceLogObject,
) -> Result<bool> {
    match &source.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r)) => {
            let result = vault.deletion_results.get(&record.request_id).or_else(|| {
                events
                    .deletion_results
                    .iter()
                    .find(|d| d.request.request_id == record.request_id)
            });
            Ok(result.map_or_else(
                || {
                    events
                        .requests
                        .contains(&VaultActionRequestEvent::DeleteSecret(r.clone()))
                },
                |result| result.request == *r,
            ))
        }
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(_))
        | VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_)) => {
            if let Some(binding) = events
                .liveboard_removal_pending
                .iter()
                .find(|b| b.request_id == record.request_id)
            {
                return Ok(record.kind == VerifiedProducerKind::RemoveRequest
                    && binding.request.sender.user_data.device.device_id == record.actor_id
                    && record.subject
                        == VerifiedProducerSubject::Device(
                            binding.request.target.user_data.device.device_id.clone(),
                        ));
            }
            Ok(events.liveboard_removal_results.iter().any(|r| r.request_id == record.request_id
                && r.requester_id == record.actor_id
                && record.subject == VerifiedProducerSubject::Device(r.target_id.clone())
                && r.application_state == meta_secret_core::node::db::events::vault::liveboard_metadata::RemovalApplicationState::Applied))
        }
        _ => Ok(false),
    }
}
