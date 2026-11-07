//! Raw response authorization is distinct from requester completion/consumption.
use super::server_data_sync::ServerSyncGateway;
use anyhow::{Result, ensure};
use meta_secret_core::node::api::SignedAction;
use meta_secret_core::node::common::model::{
    device::common::DeviceId,
    secret::{SecretDistributionType, SsClaim},
};
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use meta_secret_core::node::db::events::{
    generic_log_event::{GenericKvLogEvent, ObjIdExtractor},
    object_id::ArtifactId,
    shared_secret_event::SsWorkflowObject,
};
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;

impl<R: KvLogEventRepo> ServerSyncGateway<R> {
    pub(super) async fn verify_raw_recovery_response(
        &self,
        action: &SignedAction,
        event: &GenericKvLogEvent,
    ) -> Result<()> {
        let GenericKvLogEvent::SsWorkflow(workflow) = event else {
            return Ok(());
        };
        let (name, claim_id) = match workflow {
            SsWorkflowObject::Decline(response) => (
                response.value.vault_name.clone(),
                response.value.claim_id.clone(),
            ),
            SsWorkflowObject::Recovery(response) => (
                response.value.vault_name.clone(),
                response.value.claim_id.id.clone(),
            ),
            SsWorkflowObject::Distribution(_) => return Ok(()),
        };
        let log = PersistentSharedSecret::from(self.p_obj.clone())
            .get_ss_log_obj(name.clone())
            .await?;
        let claim = log
            .claims
            .get(&claim_id)
            .ok_or_else(|| anyhow::anyhow!("recovery response claim does not exist"))?;
        validate_workflow(workflow, claim, &action.signer)
    }
}

/// The same admitted claim/channel binding governs signed intake and addressed
/// delivery. The caller of intake has already verified the envelope signature.
pub(super) fn validate_workflow(
    workflow: &SsWorkflowObject,
    claim: &SsClaim,
    signer: &DeviceId,
) -> Result<()> {
    ensure!(
        claim.distribution_type == SecretDistributionType::Recover
            && &claim.sender != signer
            && claim.receivers.contains(signer),
        "recovery responder is not an addressed receiver"
    );
    let id = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| &id.distribution_id.receiver == signer)
        .ok_or_else(|| anyhow::anyhow!("recovery receiver identity is unavailable"))?;
    let descriptor = match workflow {
        SsWorkflowObject::Decline(response) => {
            ensure!(
                response.value.receiver_id == *signer
                    && response.value.vault_name == claim.vault_name,
                "recovery decline actor mismatch"
            );
            SsWorkflowDescriptor::Decline(id)
        }
        SsWorkflowObject::Recovery(response) => {
            ensure!(
                response.value.vault_name == claim.vault_name
                    && response.value.claim_id == claim.dist_claim_id
                    && response
                        .value
                        .secret_message
                        .cipher_text()
                        .channel
                        .sender()
                        .to_device_id()
                        == *signer
                    && response
                        .value
                        .secret_message
                        .cipher_text()
                        .channel
                        .receiver()
                        .to_device_id()
                        == claim.sender,
                "recovery channel or claim identity mismatch"
            );
            SsWorkflowDescriptor::Recovery(id)
        }
        SsWorkflowObject::Distribution(_) => anyhow::bail!("InvalidPublicReadRecord"),
    };
    ensure!(
        workflow.obj_id() == ArtifactId::from(descriptor),
        "recovery response stream does not match canonical claim"
    );
    Ok(())
}

pub(super) fn response_identity(
    event: &GenericKvLogEvent,
) -> Option<(
    meta_secret_core::node::common::model::vault::vault::VaultName,
    meta_secret_core::node::common::model::secret::ClaimId,
    meta_secret_core::node::db::events::vault::liveboard_metadata::DecisionOutcome,
)> {
    use meta_secret_core::node::db::events::vault::liveboard_metadata::DecisionOutcome;
    match event {
        GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Recovery(e)) => Some((
            e.value.vault_name.clone(),
            e.value.claim_id.id.clone(),
            DecisionOutcome::Accepted,
        )),
        GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Decline(e)) => Some((
            e.value.vault_name.clone(),
            e.value.claim_id.clone(),
            DecisionOutcome::Declined,
        )),
        _ => None,
    }
}
