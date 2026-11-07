//! Freeze only compact signature metadata, never a second packet-bearing action.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    api::{GuardedRemovalDecision, SignedAction, SignedActionPayload},
    common::model::{
        IdString, liveboard_producer::producer_source_commitment, user::common::UserData,
    },
    db::{
        events::{
            generic_log_event::ObjIdExtractor, local_guarded_preparation::*,
            vault::device_log_event::DeviceLogObject,
        },
        objects::local_guarded_preparation::{
            PersistentGuardedPreparation, UNAVAILABLE, local_body_commitment,
        },
        repo::{generic_db::KvLogEventRepo, persistent_credentials::PersistentCredentials},
    },
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    pub(super) async fn freeze_guarded_source(
        &self,
        event: DeviceLogObject,
        user: &UserData,
        request_id: String,
    ) -> Result<SignedAction> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let witness = store
            .load(&user.user_id(), &request_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        let origin = witness
            .0
            .value
            .origin
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(
            origin.event_id == event.obj_id()
                && origin.body_commitment == producer_source_commitment(&event)?
                && matches!(
                    witness.0.value.phase,
                    GuardedPreparationPhase::Staged
                        | GuardedPreparationPhase::Queued
                        | GuardedPreparationPhase::Signed
                        | GuardedPreparationPhase::AcceptedPromoting
                        | GuardedPreparationPhase::AcceptedPublished
                ),
            UNAVAILABLE
        );
        super::guarded_preparation_source::validate_complete_source(
            &witness.0.value,
            Some(&event),
        )?;
        let credentials = PersistentCredentials {
            p_obj: self.p_obj.clone(),
            master_key: self.master_key.clone(),
        }
        .get_user_creds()
        .await?
        .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(credentials.user() == *user, UNAVAILABLE);
        let payload = SignedActionPayload::GuardedRemovalDecision(GuardedRemovalDecision {
            version: 1,
            request_id,
            event,
        });
        if let Some(signature) = &origin.business_signature {
            let action = SignedAction {
                payload,
                signer: signature.signer.clone(),
                stream: signature.stream.clone(),
                nonce: signature.nonce,
                signature: signature.signature.clone(),
            };
            ensure!(
                action.signer == user.device.device_id
                    && action.stream == origin.event_id.fqdn.clone().id_str()
                    && action.nonce == origin.event_id.id.curr as u64
                    && local_body_commitment(&action)? == signature.signed_action_hash,
                UNAVAILABLE
            );
            action.verify(&user.device.keys.dsa_pk)?;
            return Ok(action);
        }
        ensure!(
            witness.0.value.payload.is_some()
                && matches!(
                    witness.0.value.phase,
                    GuardedPreparationPhase::Staged | GuardedPreparationPhase::Queued
                ),
            UNAVAILABLE
        );
        let action = SignedAction::sign(
            payload,
            user.device.device_id.clone(),
            origin.event_id.fqdn.clone().id_str(),
            origin.event_id.id.curr as u64,
            &credentials.device_creds.key_manager()?.dsa,
        )?;
        let signature = GuardedBusinessSignature {
            signer: action.signer.clone(),
            stream: action.stream.clone(),
            nonce: action.nonce,
            signature: action.signature.clone(),
            signed_action_hash: local_body_commitment(&action)?,
        };
        let mut value = witness.0.value.clone();
        value
            .origin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
            .business_signature = Some(signature);
        value.phase = GuardedPreparationPhase::Signed;
        store.replace(&witness, value).await?;
        Ok(action)
    }
}
