//! Exact source acknowledgement after a durable canonical decision. A terminal
//! UUID alone is insufficient; only the original verified full envelope matches.
use super::server_data_sync::ServerSyncGateway;
use super::state_invalidation::{StateInvalidation, StateInvalidationScope};
use anyhow::{Result, ensure};
use meta_secret_core::crypto::utils::sha256_hex;
use meta_secret_core::node::api::{SignedAction, SignedActionPayload};
use meta_secret_core::node::db::actions::vault::removal_application::RemovalApplication;
use meta_secret_core::node::db::events::{
    generic_log_event::ObjIdExtractor,
    vault::{
        liveboard_metadata::{RemovalApplicationState, RemovalSourceProof},
        vault_log_event::{VaultActionEvent, VaultActionUpdateEvent},
    },
};
use meta_secret_core::node::db::objects::{
    persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
};
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::security::canonical_json;

pub(super) fn source_proof(action: &SignedAction) -> Result<RemovalSourceProof> {
    let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
        anyhow::bail!("not a guarded decision")
    };
    Ok(RemovalSourceProof {
        event_id: guard.event.obj_id(),
        signer: action.signer.clone(),
        signed_action_hash: sha256_hex(&String::from_utf8(canonical_json(action)?)?),
    })
}

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn apply_guarded_removal(&self, action: &SignedAction) -> Result<()> {
        let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
            anyhow::bail!("not a guarded decision")
        };
        let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) =
            &guard.event.0.value
        else {
            anyhow::bail!("not a removal decision")
        };
        let name = update.request.sender.user_data.vault_name();
        let p = PersistentVault::from(self.p_obj.clone());
        let proof = source_proof(action)?;
        let log = p.get_vault_log_artifact(name.clone()).await?.0.value;
        if let Some(result) = log
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == guard.request_id)
        {
            ensure!(
                result.application_state == RemovalApplicationState::Applied
                    && result.source.as_ref() == Some(&proof),
                "terminal removal does not match original authenticated source"
            );
        } else {
            RemovalApplication {
                p_obj: self.p_obj.clone(),
            }
            .apply_with_source(update, Some(proof))
            .await?;
        }
        self.p_obj.save_verified(guard.event.clone()).await?;
        let vault = p.get_vault(name.clone()).await?.to_data();
        Box::pin(PersistentSharedSecret::from(self.p_obj.clone()).purge_deleted_secrets(&vault))
            .await?;
        self.invalidation_publisher.publish(StateInvalidation::new(
            name.clone(),
            StateInvalidationScope::Devices,
        ));
        self.invalidation_publisher.publish(StateInvalidation::new(
            name,
            StateInvalidationScope::SsClaims,
        ));
        Ok(())
    }
}
