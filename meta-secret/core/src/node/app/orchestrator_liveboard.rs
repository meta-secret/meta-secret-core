//! Freeze ALL relevant outputs privately; no ordinary material is published here.
use super::*;
use crate::node::db::{
    events::{local_guarded_preparation::*, vault::vault_event::VaultObject},
    objects::local_guarded_preparation::local_body_commitment,
};

impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    pub(crate) async fn compute_guarded_preparation(
        &self,
        request_id: &str,
        update: &RemoveMemberEvent,
        base: &VaultObject,
    ) -> Result<LocalGuardedPreparation> {
        let post = base
            .0
            .value
            .clone()
            .update_membership(update.new_status.clone());
        let mut survivors = post.members();
        survivors.sort_by_key(|m| m.user().device.device_id.to_string());
        let mut passes = vec![];
        if update.decision
            == crate::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted
            && survivors.len() == 2
        {
            let claims = self.get_ss_log_data().await?;
            let context = removal_compute::RemovalPassContext {
                vault: &post,
                update,
                claims: &claims,
                is_initiator: false,
            };
            for transfer in update
                .request
                .share_transfers
                .iter()
                .filter(|transfer| &transfer.receiver == self.user_creds.device_id())
            {
                let pass = self
                    .compute_removal_pass(&context, transfer)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("GuardedPreparationUnavailable"))?;
                passes.push(pass);
            }
        }
        let claims = passes
            .iter()
            .map(|pass| GuardedClaimOwnership {
                pass_id: pass.pass_id.clone(),
                claim: pass.claim.clone(),
                ss_device_event_id: None,
                ss_log_event_id: None,
                handed_off: false,
            })
            .collect();
        let mut packets = vec![];
        for pass in &passes {
            for packet in &pass.distributions {
                let data = packet.clone().to_distribution_data()?;
                packets.push(GuardedPacketOwnership {
                    event_id: packet.obj_id(),
                    receiver_id: data
                        .secret_message
                        .cipher_text()
                        .channel
                        .receiver()
                        .to_device_id(),
                    body_commitment: local_body_commitment(packet)?,
                    handed_off: false,
                    delivered: false,
                });
            }
        }
        Ok(LocalGuardedPreparation {
            version: 1,
            owner: self.user_creds.user_id(),
            request_id: request_id.into(),
            base_vault: base.obj_id(),
            requester_id: update.request.sender.user_data.device.device_id.clone(),
            target_id: update.target.user_data.device.device_id.clone(),
            survivors: survivors
                .into_iter()
                .map(|member| member.user().clone())
                .collect(),
            phase: GuardedPreparationPhase::Staged,
            payload: Some(GuardedPreparationPayload {
                update: update.clone(),
                passes,
            }),
            origin: None,
            claims,
            packets,
            transport_noop: None,
            previous_attempts: vec![],
        })
    }
}
