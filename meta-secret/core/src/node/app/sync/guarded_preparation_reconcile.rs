//! Already-owned, local canonical proof gates ordinary frozen publication.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    common::model::{IdString, user::common::UserData},
    db::{
        events::{
            generic_log_event::{KeyExtractor, ObjIdExtractor},
            local_guarded_preparation::*,
            vault::liveboard_metadata::{
                DecisionOutcome, RemovalApplicationState, RemovalResultRecord,
            },
        },
        objects::{
            local_guarded_preparation::{
                PersistentGuardedPreparation, UNAVAILABLE, local_body_commitment,
            },
            persistent_vault::PersistentVault,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

#[path = "guarded_applied_lifetime.rs"]
mod applied_lifetime;
#[path = "guarded_packet_ownership.rs"]
mod packet_ownership;

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    async fn original_own_applied(
        &self,
        object: &LocalGuardedPreparationObject,
    ) -> Result<Option<RemovalResultRecord>> {
        let value = &object.0.value;
        let Some(origin) = &value.origin else {
            return Ok(None);
        };
        super::guarded_preparation_source::validate_complete_source(value, None)?;
        let persistent = PersistentVault::from(self.p_obj.clone());
        let log = persistent
            .get_vault_log_artifact(value.owner.vault_name.clone())
            .await?;
        log.0.value.validate_removal_bindings(true)?;
        let Some(result) = log
            .0
            .value
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == value.request_id)
        else {
            return Ok(None);
        };
        if result.application_state != RemovalApplicationState::Applied {
            return Ok(None);
        }
        let Some(signature) = &origin.business_signature else {
            anyhow::bail!(UNAVAILABLE)
        };
        ensure!(
            signature.signer == value.owner.device_id
                && signature.stream == origin.event_id.fqdn.clone().id_str()
                && signature.nonce == origin.event_id.id.curr as u64,
            UNAVAILABLE
        );
        let Some(source) = &result.source else {
            return Ok(None);
        };
        if source.event_id != origin.event_id
            || source.signer != signature.signer
            || source.signed_action_hash != signature.signed_action_hash
        {
            return Ok(None);
        }
        ensure!(
            result.requester_id == value.requester_id
                && result.target_id == value.target_id
                && result.responder_id == value.owner.device_id
                && source.signer == value.owner.device_id
                && (result.outcome == DecisionOutcome::Accepted) == origin.accepted,
            UNAVAILABLE
        );
        Ok(Some(result.clone()))
    }

    async fn own_applied(&self, object: &LocalGuardedPreparationObject) -> Result<bool> {
        let value = &object.0.value;
        if value.phase == GuardedPreparationPhase::BlockedRetiredCommand {
            ensure!(value.payload.is_none(), UNAVAILABLE);
            return Ok(false);
        }
        if self.original_own_applied(object).await?.is_none() {
            return Ok(false);
        }
        let vault = self.guarded_current_vault(object).await?.to_data();
        let members = vault.members();
        ensure!(
            members.len() == value.survivors.len()
                && members
                    .iter()
                    .all(|member| value.survivors.contains(&member.user_data)),
            UNAVAILABLE
        );
        if value.origin.as_ref().is_some_and(|origin| origin.accepted) {
            ensure!(members.len() == 2, UNAVAILABLE);
        }
        if let Some(payload) = &value.payload {
            ensure!(
                payload
                    .passes
                    .iter()
                    .all(|pass| vault.secrets.contains(&pass.pass_id)
                        && !vault.deleted_secrets.contains(&pass.pass_id)),
                UNAVAILABLE
            );
        }
        Ok(true)
    }

    pub(super) async fn reconcile_guarded_preparations(
        &self,
        user: &UserData,
        finish: bool,
    ) -> Result<()> {
        self.reconcile_guarded_owned(user, finish)
            .await
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))
    }

    async fn reconcile_guarded_owned(&self, user: &UserData, finish: bool) -> Result<()> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        for object in store.load_all(&user.user_id()).await? {
            if self.retire_applied_after_join(user, &object).await? {
                continue;
            }
            if !self.own_applied(&object).await? {
                continue;
            }
            if !object
                .0
                .value
                .origin
                .as_ref()
                .is_some_and(|origin| origin.accepted)
            {
                if object.0.value.payload.is_some() {
                    let mut value = object.0.value.clone();
                    value.payload = None;
                    value.phase = GuardedPreparationPhase::BlockedRetiredCommand;
                    store.replace(&object, value).await?;
                }
                continue;
            }
            let published = store.publish_accepted(object).await?;
            if finish {
                let vault = self.guarded_current_vault(&published).await?;
                store.finish_accepted(&published, &vault).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn guarded_packet_handed_off(
        &self,
        user: &UserData,
        packet: &crate::node::db::events::shared_secret_event::SsWorkflowObject,
    ) -> Result<bool> {
        self.guarded_packet_ack_owned(user, packet)
            .await
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))
    }

    /// An older addressed download must not replace a proven ordinary own output.
    pub(super) async fn retain_guarded_own_packet(
        &self,
        user: &UserData,
        incoming: &crate::node::db::events::shared_secret_event::SsWorkflowObject,
    ) -> Result<bool> {
        self.retain_guarded_own_packet_owned(user, incoming)
            .await
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))
    }

    async fn retain_guarded_own_packet_owned(
        &self,
        user: &UserData,
        incoming: &crate::node::db::events::shared_secret_event::SsWorkflowObject,
    ) -> Result<bool> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let records = store.load_all(&user.user_id()).await?;
        let candidates = packet_ownership::candidates(&records, user, incoming)?;
        let mut selected = None;
        for candidate in candidates {
            let packet = candidate.packet;
            if !self.own_applied(candidate.object).await? {
                ensure!(!candidate.exact_incoming, UNAVAILABLE);
                continue;
            }
            let actual = self
                .p_obj
                .repo
                .find_one(packet.event_id.clone())
                .await?
                .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?
                .shared_secret()?;
            ensure!(
                local_body_commitment(&actual)? == packet.body_commitment,
                UNAVAILABLE
            );
            packet_ownership::validate_owned_packet(user, &candidate, &actual)?;
            ensure!(packet.handed_off && selected.is_none(), UNAVAILABLE);
            ensure!(actual.key() == incoming.key(), UNAVAILABLE);
            selected = Some(!candidate.exact_incoming);
        }
        Ok(selected.unwrap_or(false))
    }

    async fn guarded_packet_ack_owned(
        &self,
        user: &UserData,
        packet: &crate::node::db::events::shared_secret_event::SsWorkflowObject,
    ) -> Result<bool> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let commitment = local_body_commitment(packet)?;
        for object in store.load_all(&user.user_id()).await? {
            if !object
                .0
                .value
                .packets
                .iter()
                .any(|p| p.event_id == packet.obj_id() && commitment == p.body_commitment)
            {
                continue;
            }
            ensure!(
                self.own_applied(&object).await?
                    && matches!(
                        object.0.value.phase,
                        GuardedPreparationPhase::AcceptedPromoting
                            | GuardedPreparationPhase::AcceptedPublished
                    ),
                UNAVAILABLE
            );
            store.acknowledge_packet(&object, packet).await?;
            return Ok(true);
        }
        Ok(false)
    }
}
