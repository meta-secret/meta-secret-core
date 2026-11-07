//! One ordered unsigned metadata observation; never a signing capability.
use super::server_data_sync::ServerSyncGateway;
use anyhow::{Result, anyhow, ensure};
use meta_secret_core::node::{
    api::ServerTailRequest,
    common::model::{
        liveboard_producer::*, user::common::UserMembership, vault::vault_data::VaultData,
    },
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            object_id::ArtifactId,
            vault::{producer_metadata::*, vault_log_event::*},
        },
        objects::{
            device_transport::DeviceTransportSlot, persistent_device_log::PersistentDeviceLog,
            persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn producer_observation(
        &self,
        request: &ServerTailRequest,
        next: Option<ArtifactId>,
    ) -> Result<Option<ProducerCanonicalObservation>> {
        let Some(input) = &request.producer_reference else {
            return Ok(None);
        };
        self.observe_producer_inner(request, input, next)
            .await
            .map(Some)
            .map_err(|_| anyhow!("InvalidProducerReference"))
    }

    async fn observe_producer_inner(
        &self,
        request: &ServerTailRequest,
        input: &ProducerReference,
        next: Option<ArtifactId>,
    ) -> Result<ProducerCanonicalObservation> {
        // This also bounds references constructed by an in-process caller. The
        // actual protocol decoder separately uses the same strict typed grammar.
        let input = ProducerReference::parse_json(&serde_json::to_string(input)?)?;
        let expected = ArtifactId::from(DeviceLogDescriptor::from(request.sender.user_id()));
        ensure!(
            input.request_stage.event_id.fqdn == expected.fqdn,
            "InvalidProducerReference"
        );
        let name = request.sender.vault_name();
        let p = PersistentVault::from(self.p_obj.clone());
        let vault = p.get_vault(name.clone()).await?.to_data();
        ensure!(
            matches!(vault.find_user(&request.sender.device.device_id),
                Some(UserMembership::Member(member))
                    if member.user_data == request.sender),
            "InvalidProducerReference"
        );
        let events = p.get_vault_log_artifact(name.clone()).await?.0.value;
        let record = events.producer_source(&input.request_stage.event_id)?;
        let normalized_next = next.unwrap_or(expected.clone());
        let mut observation = ProducerCanonicalObservation {
            version: 1,
            kind: input.kind.clone(),
            secret_id: None,
            target_device_id: None,
            request_id: None,
            request_stage: ProducerAttemptStage {
                source: input.request_stage.clone(),
                observation: ProducerSourceObservation::Unconfirmed,
            },
            decision_stage: None,
            canonical_state: ProducerCanonicalState::Unknown,
        };
        if let Some(record) = record {
            let expected_kind = match input.kind {
                ProducerKind::DeleteSecret => VerifiedProducerKind::DeleteRequest,
                ProducerKind::RemoveDevice => VerifiedProducerKind::RemoveRequest,
            };
            ensure!(
                record.kind == expected_kind
                    && record.actor_id == request.sender.device.device_id
                    && record.vault_name == name
                    && record.body_commitment == input.request_stage.body_commitment
                    && input
                        .request_id
                        .as_ref()
                        .is_none_or(|id| *id == record.request_id),
                "InvalidProducerReference"
            );
            observation.canonical_state = canonical_state(&events, &vault, record)?;
            observation.request_id = Some(record.request_id.clone());
            match &record.subject {
                VerifiedProducerSubject::Secret(id) => observation.secret_id = Some(id.clone()),
                VerifiedProducerSubject::Device(id) => {
                    observation.target_device_id = Some(id.to_string())
                }
            }
            observation.request_stage.observation = self.source_observation(record).await?;
            if let Some(decision) = &input.decision_stage {
                ensure!(
                    decision.event_id.fqdn == expected.fqdn,
                    "InvalidProducerReference"
                );
                let update = events
                    .producer_source(&decision.event_id)?
                    .ok_or_else(|| anyhow!("InvalidProducerReference"))?;
                ensure!(
                    update.kind == VerifiedProducerKind::LocalRemoveAccept
                        && update.actor_id == record.actor_id
                        && update.vault_name == name
                        && update.request_id == record.request_id
                        && update.subject == record.subject
                        && update.body_commitment == decision.body_commitment,
                    "InvalidProducerReference"
                );
                observation.decision_stage = Some(ProducerAttemptStage {
                    source: decision.clone(),
                    observation: self.source_observation(update).await?,
                });
            }
        } else {
            ensure!(input.decision_stage.is_none(), "InvalidProducerReference");
            let raw = self
                .p_obj
                .repo
                .find_one(input.request_stage.event_id.clone())
                .await?;
            // NEXT==N is a free raw position, not an acknowledgement. Past N
            // without exact admission proof remains unsupported/Unknown.
            if raw.is_none()
                && normalized_next.fqdn == expected.fqdn
                && normalized_next.id.curr <= input.request_stage.event_id.id.curr
            {
                observation.request_stage.observation = ProducerSourceObservation::Absent;
            }
        }
        Ok(observation)
    }

    async fn source_observation(
        &self,
        source: &VerifiedProducerSourceRef,
    ) -> Result<ProducerSourceObservation> {
        let owner = meta_secret_core::node::common::model::user::common::UserId {
            vault_name: source.vault_name.clone(),
            device_id: source.actor_id.clone(),
        };
        let Some(raw) = PersistentDeviceLog::from(self.p_obj.clone())
            .transport_slot(&owner, source.source_event_id.clone())
            .await?
        else {
            return Ok(ProducerSourceObservation::Admitted);
        };
        let DeviceTransportSlot::Business(raw) = raw else {
            // The canonical admission is authoritative, but a technical slot
            // can never acknowledge the original Business bytes.
            return Ok(ProducerSourceObservation::Admitted);
        };
        if producer_body_commitment(&raw)? == source.body_commitment {
            return Ok(ProducerSourceObservation::RawAcknowledged);
        }
        // Trusted purge changes retained Remove packets, not their immutable
        // admitted origin. Never re-hash the scrubbed body into a new identity.
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(source.vault_name.clone())
            .await?
            .to_data();
        let (actor, subject, transfers) = match &raw.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) => {
                (&r.sender, &r.target, &r.share_transfers)
            }
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) => {
                (&u.sender, &u.target, &u.share_transfers)
            }
            _ => anyhow::bail!("InvalidProducerReference"),
        };
        ensure!(
            !vault.deleted_secrets.is_empty()
                && actor.user_data.device.device_id == source.actor_id
                && actor.user_data.vault_name() == source.vault_name
                && source.subject
                    == VerifiedProducerSubject::Device(subject.user_data.device.device_id.clone())
                && transfers.iter().all(|transfer| !vault
                    .deleted_secrets
                    .iter()
                    .any(|pass| pass.id == transfer.pass_id.id)),
            "InvalidProducerReference"
        );
        Ok(ProducerSourceObservation::Redacted)
    }
}

fn canonical_state(
    events: &VaultActionEvents,
    vault: &VaultData,
    source: &VerifiedProducerSourceRef,
) -> Result<ProducerCanonicalState> {
    match source.kind {
        VerifiedProducerKind::DeleteRequest => {
            let result = vault.deletion_results.get(&source.request_id).or_else(|| {
                events
                    .deletion_results
                    .iter()
                    .find(|r| r.request.request_id == source.request_id)
            });
            if let Some(result) = result {
                ensure!(
                    source.subject
                        == VerifiedProducerSubject::Secret(result.request.pass_id.id_str())
                        && source.actor_id == result.request.sender.user_data.device.device_id
                        && source.vault_name == result.request.sender.user_data.vault_name(),
                    "InvalidProducerReference"
                );
                return Ok(if result.accepted {
                    ProducerCanonicalState::Applied
                } else {
                    ProducerCanonicalState::Declined
                });
            }
            ensure!(
                events.requests.iter().any(
                    |r| matches!(r, VaultActionRequestEvent::DeleteSecret(p)
                if p.request_id == source.request_id
                    && source.subject == VerifiedProducerSubject::Secret(p.pass_id.id_str())
                    && p.sender.user_data.device.device_id == source.actor_id)
                ),
                "InvalidProducerReference"
            );
            Ok(ProducerCanonicalState::Pending)
        }
        VerifiedProducerKind::RemoveRequest | VerifiedProducerKind::LocalRemoveAccept => {
            if let Some(result) = events
                .liveboard_removal_results
                .iter()
                .find(|r| r.request_id == source.request_id)
            {
                ensure!(
                    source.actor_id == result.requester_id
                        && source.subject
                            == VerifiedProducerSubject::Device(result.target_id.clone()),
                    "InvalidProducerReference"
                );
                use meta_secret_core::node::db::events::vault::liveboard_metadata::{
                    DecisionOutcome, RemovalApplicationState,
                };
                return Ok(
                    if result.application_state == RemovalApplicationState::Prepared {
                        ProducerCanonicalState::Prepared
                    } else if result.outcome == DecisionOutcome::Declined {
                        ProducerCanonicalState::Declined
                    } else {
                        ProducerCanonicalState::Applied
                    },
                );
            }
            ensure!(
                events
                    .liveboard_removal_pending
                    .iter()
                    .any(|r| r.request_id == source.request_id
                        && r.request.sender.user_data.device.device_id == source.actor_id
                        && source.subject
                            == VerifiedProducerSubject::Device(
                                r.request.target.user_data.device.device_id.clone()
                            )),
                "InvalidProducerReference"
            );
            Ok(ProducerCanonicalState::Pending)
        }
    }
}
