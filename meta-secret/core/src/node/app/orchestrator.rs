use crate::crypto::utils::{Id48bit, sha256_hex};
use crate::node::app::sync::environment::{SELECTED_SERVER_ENVIRONMENT, ServerEnvironment};
use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::{PlainPassInfo, SecurePassInfo};
use crate::node::common::model::secret::{
    ClaimId, SecretDistributionData, SecretDistributionType, SsClaim, SsDeclineData,
    SsDistributionCompositeStatus, SsDistributionId, SsLogData,
};
use crate::node::common::model::user::common::{UserDataMember, UserMembership};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::actions::sign_up::join::{JoinAction, JoinActionUpdate};
use crate::node::db::actions::sign_up::remove::{RemoveAction, RemoveActionUpdate};
use crate::node::db::descriptors::shared_secret_descriptor::SsDeviceLogDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::events::generic_log_event::ObjIdExtractor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::{SsDeviceLogObject, SsWorkflowObject};
use crate::node::db::events::vault::vault_log_event::{
    JoinClusterEvent, RemoveMemberEvent, RemoveMemberRequestEvent, RemoveMemberShareTransfer,
    VaultActionRequestEvent, VaultActionUpdateEvent, VaultLogObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::recover_from_shares;
use crate::secret::shared_secret::{PlainText, UserShareDto};
use crate::secret::split2;
use anyhow::Result;
use anyhow::bail;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info};

#[cfg(target_os = "android")]
fn core_diag(message: &str) {
    if !matches!(SELECTED_SERVER_ENVIRONMENT, ServerEnvironment::Local) {
        return;
    }
    use std::ffi::CString;
    use std::os::raw::c_char;

    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: i32, tag: *const c_char, text: *const c_char) -> i32;
    }

    let Ok(message) = CString::new(message) else {
        return;
    };
    unsafe {
        __android_log_write(4, c"MetaSecretE2E".as_ptr().cast(), message.as_ptr().cast());
    }
}

fn core_diag_error_class(error: &impl std::fmt::Display) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("decrypt") || message.contains("encrypt") || message.contains("crypto") {
        "crypto"
    } else if message.contains("share") || message.contains("threshold") {
        "share"
    } else if message.contains("parse") || message.contains("json") || message.contains("decode") {
        "decode"
    } else if message.contains("save") || message.contains("database") || message.contains("repo") {
        "storage"
    } else {
        "other"
    }
}

/// Return a shared claim ID for the new share generation created by a canonical
/// membership event. Every current member can derive the same value from the
/// previous generation and event, so concurrent reconcilers do not create
/// competing polynomials with unrelated claim IDs.
fn reshare_generation_claim_id(previous: &ClaimId, event_key: &str) -> ClaimId {
    let digest = sha256_hex(&format!("{}:{event_key}", previous.0.text));
    ClaimId::from(Id48bit {
        text: format!("{}-{}-{}", &digest[0..4], &digest[4..8], &digest[8..12]),
    })
}

/// Stable, non-reversible diagnostic label for correlating share generations
/// without logging claim identifiers or any share material.
fn claim_generation_alias(claim_id: &ClaimId) -> String {
    sha256_hex(&claim_id.0.text)[..12].to_string()
}

fn device_diagnostic_alias(device_id: &DeviceId) -> String {
    sha256_hex(&device_id.to_string())[..12].to_string()
}

#[cfg(not(target_os = "android"))]
fn core_diag(message: &str) {
    if matches!(SELECTED_SERVER_ENVIRONMENT, ServerEnvironment::Local) {
        println!("{message}");
    }
}

/// Contains business logic of secrets management and login/sign-up actions.
/// Orchestrator is in charge of what is meta secret is made for (the most important part of the app).
/// 1. Passwordless login
/// 2. Decentralized User Management
/// 3. Secret orchestration
pub struct MetaOrchestrator<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user_creds: UserCreds,
}

impl<Repo: KvLogEventRepo> MetaOrchestrator<Repo> {
    /// Accept all requests automatically
    pub async fn orchestrate(&self) -> Result<()> {
        let member = self.get_member().await?;
        let maybe_vault_log_event = self.get_vault_log_event(&member).await?;

        let Some(VaultLogObject(action_event)) = maybe_vault_log_event else {
            return Ok(());
        };

        let vault_actions = action_event.value;

        for request in vault_actions.requests {
            match request {
                VaultActionRequestEvent::JoinCluster(join_request) => {
                    self.update_membership(join_request, JoinActionUpdate::Accept)
                        .await?;
                }
                VaultActionRequestEvent::RemoveMember(_)
                | VaultActionRequestEvent::DeleteSecret(_) => {
                    // Removal requires an explicit user action; never auto-approve it.
                }
                VaultActionRequestEvent::AddMetaPass(_) => {
                    //skip
                }
            }
        }

        // shared secret actions
        let ss_log_data = self.get_ss_log_data().await?;

        for (_, claim) in ss_log_data.claims {
            self.accept_recover(claim.id).await?;
        }

        Ok(())
    }

    pub async fn accept_recover(&self, claim_id: ClaimId) -> Result<()> {
        let member = self.get_member().await?;
        let vault = self.get_vault(member).await?;

        // shared secret actions
        let ss_log_data = self.get_ss_log_data().await?;

        for (_, claim) in ss_log_data.claims {
            match claim.distribution_type {
                SecretDistributionType::Split => {
                    //skip
                }
                SecretDistributionType::Recover => {
                    if claim_id.eq(&claim.id) {
                        self.handle_recover(vault.clone(), claim).await?;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn decline_recover(&self, claim_id: ClaimId) -> Result<()> {
        println!("🦀 Orchestrator: decline claim_id: {:?}", claim_id);
        let local_device_id = self.user_creds.device_id().clone();

        let ss_log_data = self.get_ss_log_data().await?;

        let Some(claim) = ss_log_data.claims.get(&claim_id) else {
            bail!("Claim not found: {:?}", claim_id);
        };

        let mut updated_claim = claim.clone();
        println!("🦀 Orchestrator: local_device_id: {:?}", local_device_id);
        updated_claim.status = updated_claim.status.decline(local_device_id.clone());
        println!(
            "🦀 Orchestrator: updated_claim status: {:?}",
            updated_claim.status
        );

        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        p_ss.save_ss_log_event(updated_claim).await?;

        for recovery_db_id in claim.recovery_db_ids() {
            if recovery_db_id.distribution_id.receiver.eq(&local_device_id) {
                let decline_data = SsDeclineData {
                    vault_name: claim.vault_name.clone(),
                    claim_id: claim.id.clone(),
                    receiver_id: local_device_id.clone(),
                };
                let key = KvKey::from(SsWorkflowDescriptor::Decline(recovery_db_id.clone()));
                let decline_wf = SsWorkflowObject::Decline(KvLogEvent {
                    key,
                    value: decline_data,
                });
                self.p_obj.repo.save(decline_wf).await?;
                break;
            }
        }

        debug!("Declined recovery request for claim: {:?}", claim_id);

        Ok(())
    }

    pub async fn update_membership(
        &self,
        join_request: JoinClusterEvent,
        upd: JoinActionUpdate,
    ) -> Result<()> {
        let member = self.get_member().await?;
        let vault = self.get_vault(member.clone()).await?;
        let maybe_vault_log_event = self.get_vault_log_event(&member).await?;

        let Some(VaultLogObject(action_event)) = maybe_vault_log_event else {
            return Ok(());
        };

        let vault_actions = action_event.value;

        for request in vault_actions.requests {
            match request {
                VaultActionRequestEvent::JoinCluster(db_join_request) => {
                    if join_request.candidate == db_join_request.candidate
                        && (join_request.request_id.is_empty()
                            || join_request.request_id == db_join_request.request_id)
                    {
                        let join_action = JoinAction {
                            p_obj: self.p_obj.clone(),
                            member: VaultMember {
                                member: member.clone(),
                                vault: vault.clone(),
                            },
                        };

                        join_action
                            .update(db_join_request.clone(), upd.clone())
                            .await?;

                        if let JoinActionUpdate::Accept = upd {
                            let redistribution_vault = vault.clone().update_membership(
                                UserMembership::Member(UserDataMember {
                                    user_data: db_join_request.candidate.clone(),
                                }),
                            );
                            self.redistribute_existing_secrets(
                                &redistribution_vault,
                                &db_join_request,
                            )
                            .await?;
                        }
                    }
                }
                VaultActionRequestEvent::RemoveMember(_)
                | VaultActionRequestEvent::DeleteSecret(_) => {
                    // Handled by `request_remove_member` / `accept_remove_member`.
                }
                VaultActionRequestEvent::AddMetaPass(_) => {
                    //Ignore server side events (no need approval)
                }
            }
        }

        Ok(())
    }

    /// Create a signed removal request. In a two-device vault the same local
    /// confirmation immediately completes the operation; with three devices a
    /// different member must call `accept_remove_member`.
    pub async fn request_remove_member(
        &self,
        target: UserDataMember,
    ) -> Result<RemoveMemberRequestEvent> {
        let member = self.get_member().await?;
        let vault = self.get_vault(member.clone()).await?;
        let action = RemoveAction {
            p_obj: self.p_obj.clone(),
            member: VaultMember {
                member,
                vault: vault.clone(),
            },
        };
        let request_share_transfers = if vault.members().len() >= 3 {
            self.build_local_share_transfers_for_removal(&vault, &target)
                .await?
        } else {
            vec![]
        };
        let request = action.request(target, request_share_transfers).await?;
        if vault.members().len() == 2 {
            let update = action
                .update(request.clone(), RemoveActionUpdate::Accept, vec![])
                .await?;
            let single_device_vault = vault.update_membership(update.new_status);
            self.canonicalize_single_device_claims(&single_device_vault)
                .await?;
        }
        Ok(request)
    }

    /// Approve the pending removal for the target device. Core checks that the
    /// local device is a different current member when three devices remain.
    pub async fn accept_remove_member(&self, target_device_id: String) -> Result<()> {
        let member = self.get_member().await?;
        let vault = self.get_vault(member.clone()).await?;
        let Some(VaultLogObject(log)) = self.get_vault_log_event(&member).await? else {
            bail!("Removal request not found");
        };
        let request = log
            .value
            .requests
            .into_iter()
            .find_map(|request| match request {
                VaultActionRequestEvent::RemoveMember(request)
                    if request.target.user_data.device.device_id.to_string()
                        == target_device_id =>
                {
                    Some(request)
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("Removal request not found"))?;
        let action = RemoveAction {
            p_obj: self.p_obj.clone(),
            member: VaultMember {
                member,
                vault: vault.clone(),
            },
        };
        let request_share_transfers = if vault.members().len() >= 3 {
            let local_device_id = self.user_creds.device_id();
            request
                .share_transfers
                .iter()
                .filter(|transfer| &transfer.receiver == local_device_id)
                .cloned()
                .collect()
        } else {
            vec![]
        };
        core_diag(&format!(
            "[CORE_DIAG] removal approval actor_type={} current_members={} matching_local_transfers={} request_transfers={}",
            self.user_creds.device_creds.device.device_type.as_str(),
            vault.members().len(),
            request_share_transfers.len(),
            request.share_transfers.len()
        ));
        let update = action
            .update(request, RemoveActionUpdate::Accept, vec![])
            .await?;

        if vault.members().len() >= 3 {
            let post_removal_vault = vault.update_membership(update.new_status.clone());
            let removal_cfg = post_removal_vault.sss_cfg();
            core_diag(&format!(
                "[CORE_DIAG] removal post_membership actor_type={} remaining_members={} threshold={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                post_removal_vault.members().len(),
                removal_cfg.threshold
            ));
            self.reshare_after_removal(
                &post_removal_vault,
                &update,
                request_share_transfers,
                false,
            )
            .await?;
        }
        Ok(())
    }

    /// Decline a pending removal request without changing membership or Key Shares.
    /// The signed terminal response consumes the exact pending request on the server.
    pub async fn decline_remove_member(&self, target_device_id: String) -> Result<()> {
        let member = self.get_member().await?;
        let vault = self.get_vault(member.clone()).await?;
        let Some(VaultLogObject(log)) = self.get_vault_log_event(&member).await? else {
            bail!("Removal request not found");
        };
        let request = log
            .value
            .requests
            .into_iter()
            .find_map(|request| match request {
                VaultActionRequestEvent::RemoveMember(request)
                    if request.target.user_data.device.device_id.to_string()
                        == target_device_id =>
                {
                    Some(request)
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("Removal request not found"))?;
        RemoveAction {
            p_obj: self.p_obj.clone(),
            member: VaultMember { member, vault },
        }
        .update(request, RemoveActionUpdate::Decline, vec![])
        .await?;
        Ok(())
    }

    /// Prepare the initiator's local shares for the approver. This lets the
    /// approver perform the canonical resharing when the initiator did not
    /// create a particular secret.
    async fn build_local_share_transfers_for_removal(
        &self,
        vault: &VaultData,
        target: &UserDataMember,
    ) -> Result<Vec<RemoveMemberShareTransfer>> {
        let local_device_id = self.user_creds.device_id().clone();
        let approvers: Vec<UserDataMember> = vault
            .members()
            .into_iter()
            .filter(|member| {
                member.user_data.device.device_id != local_device_id
                    && member.user_data.device.device_id != target.user_data.device.device_id
            })
            .collect();
        let mut transfers = Vec::new();
        for approver in approvers {
            for pass_id in vault.secrets.iter() {
                transfers.push(self.encrypt_local_share_for(pass_id, &approver).await?);
            }
        }
        Ok(transfers)
    }

    async fn encrypt_local_share_for(
        &self,
        pass_id: &crate::node::common::model::meta_pass::MetaPasswordId,
        receiver: &UserDataMember,
    ) -> Result<RemoveMemberShareTransfer> {
        let local_device_id = self.user_creds.device_id().clone();
        let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: local_device_id,
        });
        let Some(SsWorkflowObject::Distribution(distribution)) =
            self.p_obj.find_tail_event(desc).await?
        else {
            bail!("Local share for {} is missing", pass_id.name);
        };

        let plain = distribution
            .value
            .secret_message
            .cipher_text()
            .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
        let share = UserShareDto::try_from(&plain.msg)?;
        let encrypted = self
            .user_creds
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(
                PlainText::from(share.as_json()?),
                &receiver.user_data.device.keys.transport_pk,
            )?;

        Ok(RemoveMemberShareTransfer {
            pass_id: pass_id.clone(),
            receiver: receiver.user_data.device.device_id.clone(),
            claim_id: Some(distribution.value.claim_id.id.clone()),
            encrypted_share: serde_json::to_string(&EncryptedMessage::CipherShare {
                share: encrypted,
            })?,
        })
    }

    /// Compatibility fallback for a pending 3→2 resharing carried by a signed
    /// approval update. New approvals are reshared immediately by the approver
    /// in `accept_remove_member`, before the completed Vault event is cleared.
    pub async fn finalize_pending_removal_resharing(&self) -> Result<()> {
        // The removed device receives the same signed update and immediately
        // becomes an Outsider/Declined. It must still be able to build its
        // next app state, so a non-member simply has nothing to reshare.
        let p_vault = PersistentVault::from(self.p_obj());
        let vault_status = p_vault.find(self.user_creds.user()).await?;
        let VaultStatus::Member(member) = vault_status else {
            return Ok(());
        };
        let vault = p_vault
            .get_vault(member.user_data.vault_name())
            .await?
            .to_data();
        if vault.members().len() == 1 {
            self.canonicalize_single_device_claims(&vault).await?;
            return Ok(());
        }
        if vault.members().len() != 2 {
            return Ok(());
        }
        let Some(VaultLogObject(log)) = self.get_vault_log_event(&member).await? else {
            return Ok(());
        };
        let local_device_id = self.user_creds.device_id().clone();
        // The approver is the sole owner of 3→2 resharing: it applies the
        // accepted removal and writes the new shares before the event is
        // cleared. Retrying from the initiator would independently generate
        // another random polynomial from the request's old K=2 transfers.
        // Those two valid but different resharing generations can then be
        // mixed across devices and fail to recover. Only the approver retries
        // its own interrupted local persistence. A VaultLog can retain older
        // accepted removal updates across a later rejoin; check each retained
        // update against the current local shares and skip stale generations.
        let retry_candidates: Vec<RemoveMemberEvent> = log
            .value
            .updates
            .into_iter()
            .filter_map(|event| match event {
                VaultActionUpdateEvent::RemoveMember(update)
                    if should_retry_removal_reshare(
                        &local_device_id,
                        &update.sender.user_data.device.device_id,
                        !update.request.share_transfers.is_empty(),
                    ) =>
                {
                    Some(update)
                }
                _ => None,
            })
            .collect();
        core_diag(&format!(
            "[CORE_DIAG] removal retry candidates actor_type={} count={}",
            self.user_creds.device_creds.device.device_type.as_str(),
            retry_candidates.len()
        ));
        if retry_candidates.is_empty() {
            return Ok(());
        }

        for update in retry_candidates {
            let transfers: Vec<RemoveMemberShareTransfer> = update
                .request
                .share_transfers
                .iter()
                .filter(|transfer| transfer.receiver == local_device_id)
                .cloned()
                .collect();
            if transfers.is_empty() {
                continue;
            }
            let mut topology_already_current = true;
            for transfer in &transfers {
                if !self
                    .removal_claim_topology_matches(&vault, &local_device_id, &transfer.pass_id)
                    .await?
                {
                    topology_already_current = false;
                    break;
                }
            }
            if topology_already_current {
                return Ok(());
            }
            if !self
                .removal_transfers_match_local_generation(&transfers, &local_device_id)
                .await?
            {
                core_diag(&format!(
                    "[CORE_DIAG] removal retry skipped actor_type={} reason=transfer_generation_mismatch",
                    self.user_creds.device_creds.device.device_type.as_str()
                ));
                continue;
            }
            core_diag(&format!(
                "[CORE_DIAG] removal retry selected actor_type={} transfer_count={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                transfers.len()
            ));
            self.reshare_after_removal(&vault, &update, transfers, false)
                .await?;
            return Ok(());
        }
        bail!(
            "Pending removal resharing has no transfer compatible with the current Key Share generation"
        )
    }

    /// A VaultLog may retain completed removal updates after a later rejoin.
    /// Before retrying any candidate, verify that every encrypted request share
    /// can recover with the approver's current local share. This selects a
    /// coherent generation without relying on HashSet iteration order or
    /// exposing share data in logs.
    async fn removal_transfers_match_local_generation(
        &self,
        transfers: &[RemoveMemberShareTransfer],
        local_device_id: &DeviceId,
    ) -> Result<bool> {
        for transfer in transfers {
            let local_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: transfer.pass_id.clone(),
                receiver: local_device_id.clone(),
            });
            let Some(SsWorkflowObject::Distribution(local_distribution)) =
                self.p_obj.find_tail_event(local_desc).await?
            else {
                return Ok(false);
            };
            if transfer
                .claim_id
                .as_ref()
                .is_some_and(|claim_id| claim_id != &local_distribution.value.claim_id.id)
            {
                return Ok(false);
            }
            let local_plain = local_distribution
                .value
                .secret_message
                .cipher_text()
                .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
            let local_share = UserShareDto::try_from(&local_plain.msg)?;

            let transfer_message: EncryptedMessage =
                serde_json::from_str(&transfer.encrypted_share)?;
            let transfer_plain = transfer_message
                .cipher_text()
                .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
            let transferred_share = UserShareDto::try_from(&transfer_plain.msg)?;
            if recover_from_shares(vec![local_share, transferred_share]).is_err() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn removal_claim_topology_matches(
        &self,
        vault: &VaultData,
        local_device_id: &DeviceId,
        pass_id: &crate::node::common::model::meta_pass::MetaPasswordId,
    ) -> Result<bool> {
        let mut members = vault.members();
        members.sort_by_key(|member| member.user().device.device_id.to_string());
        let remaining_ids: HashSet<_> = members
            .iter()
            .map(|member| member.user().device.device_id.clone())
            .collect();
        let claim_events = self
            .p_obj
            .get_object_events_from_beginning(SsDeviceLogDescriptor::from(local_device_id.clone()))
            .await?;
        let claim = claim_events
            .into_iter()
            .map(|event: SsDeviceLogObject| event.to_distribution_request())
            .rev()
            .find(|claim| {
                claim.distribution_type == SecretDistributionType::Split
                    && claim.dist_claim_id.pass_id == *pass_id
            });
        let matches = claim.as_ref().is_some_and(|claim| {
            claim.receivers.len() == remaining_ids.len().saturating_sub(1)
                && claim
                    .receivers
                    .iter()
                    .all(|receiver| remaining_ids.contains(receiver))
        });
        core_diag(&format!(
            "[CORE_DIAG] removal retry topology actor_type={} found_claim={} receivers={} remaining_members={} matches={}",
            self.user_creds.device_creds.device.device_type.as_str(),
            claim.is_some(),
            claim
                .as_ref()
                .map(|claim| claim.receivers.len())
                .unwrap_or(0),
            remaining_ids.len(),
            matches
        ));
        Ok(matches)
    }

    /// Make the sole remaining member the canonical sender for every local
    /// `k=1` Key Share. A later Join can then redistribute every Secret even
    /// when its previous sender was the device removed by the `2 → 1` step.
    async fn canonicalize_single_device_claims(&self, vault: &VaultData) -> Result<()> {
        let members = vault.members();
        if members.len() != 1 {
            bail!("Single-device claim reconciliation requires exactly one vault member");
        }

        let local_device_id = self.user_creds.device_id().clone();
        if members[0].user().device.device_id != local_device_id {
            bail!("Single-device claim reconciliation requires the local vault member");
        }

        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let ss_log_data = self.get_ss_log_data().await?;
        let local_claim_events = self
            .p_obj
            .get_object_events_from_beginning(SsDeviceLogDescriptor::from(local_device_id.clone()))
            .await?;
        let local_claims: Vec<SsClaim> = local_claim_events
            .into_iter()
            .map(|obj: SsDeviceLogObject| obj.to_distribution_request())
            .collect();
        let mut updated_claims = 0usize;

        for pass_id in &vault.secrets {
            let local_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: local_device_id.clone(),
            });
            let Some(SsWorkflowObject::Distribution(local_distribution)) =
                self.p_obj.find_tail_event(local_desc).await?
            else {
                bail!("Local Key Share is missing during single-device reconciliation");
            };

            let local_plain = local_distribution
                .value
                .secret_message
                .cipher_text()
                .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
            let local_share = UserShareDto::try_from(&local_plain.msg)?;
            let threshold = local_share
                .share_blocks
                .first()
                .map(|block| block.config.threshold)
                .unwrap_or(0);
            if threshold != 1 {
                bail!("Single-device reconciliation requires a k=1 Key Share");
            }

            let claim_id = &local_distribution.value.claim_id.id;
            let mut split_claim = ss_log_data
                .claims
                .get(claim_id)
                .cloned()
                .or_else(|| {
                    local_claims
                        .iter()
                        .find(|claim| claim.id == *claim_id)
                        .cloned()
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("Split claim is missing during single-device reconciliation")
                })?;
            if split_claim.distribution_type != SecretDistributionType::Split
                || split_claim.dist_claim_id.pass_id != *pass_id
            {
                bail!("Local Key Share references an invalid split claim");
            }

            let already_canonical = split_claim.sender == local_device_id
                && split_claim.receivers.is_empty()
                && split_claim.status.statuses.is_empty();
            if already_canonical {
                continue;
            }

            split_claim.sender = local_device_id.clone();
            split_claim.receivers.clear();
            split_claim.status = SsDistributionCompositeStatus::from(vec![]);
            split_claim.client_status = None;
            p_ss.save_claim_in_ss_device_log(split_claim.clone())
                .await?;
            p_ss.save_ss_log_event(split_claim).await?;
            updated_claims += 1;
        }

        info!(
            local_device = ?local_device_id,
            updated_claims,
            "Canonicalized single-device split claims after member removal"
        );
        Ok(())
    }

    async fn reshare_after_removal(
        &self,
        vault: &VaultData,
        update: &RemoveMemberEvent,
        transfers: Vec<RemoveMemberShareTransfer>,
        is_initiator: bool,
    ) -> Result<()> {
        let local_device_id = self.user_creds.device_id().clone();
        let mut members = vault.members();
        members.sort_by_key(|member| member.user().device.device_id.to_string());
        let remaining_ids: HashSet<_> = members
            .iter()
            .map(|member| member.user().device.device_id.clone())
            .collect();
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let mut ss_log_data = self.get_ss_log_data().await?;
        let removal_generation_event = serde_json::to_string(update)?;
        let reshare_cfg = vault.sss_cfg();
        core_diag(&format!(
            "[CORE_DIAG] removal reshare start actor_type={} is_initiator={} remaining_members={} transfers={} threshold={}",
            self.user_creds.device_creds.device.device_type.as_str(),
            is_initiator,
            members.len(),
            transfers.len(),
            reshare_cfg.threshold
        ));

        for transfer in &transfers {
            core_diag(&format!(
                "[CORE_DIAG] removal reshare transfer actor_type={} claim_count={} is_initiator={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                ss_log_data.claims.len(),
                is_initiator
            ));
            if is_initiator {
                let initiator_id = &update.request.sender.user_data.device.device_id;
                let initiator_owns_claim = ss_log_data.claims.values().any(|claim| {
                    claim.distribution_type == SecretDistributionType::Split
                        && &claim.sender == initiator_id
                        && claim.dist_claim_id.pass_id == transfer.pass_id
                });
                if !initiator_owns_claim {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare skipped actor_type={} reason=initiator_does_not_own_claim",
                        self.user_creds.device_creds.device.device_type.as_str()
                    ));
                    continue;
                }
            }

            let local_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: transfer.pass_id.clone(),
                receiver: local_device_id.clone(),
            });
            core_diag("[CORE_DIAG] removal reshare local_distribution_lookup begin");
            let local_distribution = match self.p_obj.find_tail_event(local_desc).await {
                Ok(Some(SsWorkflowObject::Distribution(distribution))) => distribution,
                Ok(_) => {
                    core_diag(
                        "[CORE_DIAG] removal reshare local_distribution_lookup result=missing",
                    );
                    bail!("Local share for {} is missing", transfer.pass_id.name);
                }
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare local_distribution_lookup result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error);
                }
            };
            core_diag("[CORE_DIAG] removal reshare local_distribution_lookup result=found");
            let generation_matches_transfer = transfer
                .claim_id
                .as_ref()
                .is_none_or(|claim_id| claim_id == &local_distribution.value.claim_id.id);
            core_diag(&format!(
                "[CORE_DIAG] removal reshare generations local={} transfer={} matches={}",
                claim_generation_alias(&local_distribution.value.claim_id.id),
                transfer
                    .claim_id
                    .as_ref()
                    .map(claim_generation_alias)
                    .as_deref()
                    .unwrap_or("legacy-unknown"),
                generation_matches_transfer
            ));
            if !generation_matches_transfer {
                bail!("Removal transfer share belongs to a different Split generation");
            }
            let local_plain = match local_distribution
                .value
                .secret_message
                .cipher_text()
                .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)
            {
                Ok(plain) => plain,
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare local_distribution_decrypt result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error);
                }
            };
            core_diag("[CORE_DIAG] removal reshare local_distribution_decrypt result=ok");
            let local_share = match UserShareDto::try_from(&local_plain.msg) {
                Ok(share) => share,
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare local_distribution_parse result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error.into());
                }
            };
            core_diag("[CORE_DIAG] removal reshare local_distribution_parse result=ok");
            let local_threshold = local_share
                .share_blocks
                .first()
                .map(|block| block.config.threshold)
                .unwrap_or(0);

            // Select the Split claim referenced by the actual local share. The
            // claim map contains historical generations, and the actor may be a
            // receiver whose device log does not contain the current sender claim.
            let Some(previous_split_claim) = ss_log_data
                .claims
                .get(&local_distribution.value.claim_id.id)
                .cloned()
            else {
                bail!("Current local share references a missing Split claim");
            };
            if previous_split_claim.distribution_type != SecretDistributionType::Split
                || previous_split_claim.dist_claim_id.pass_id != transfer.pass_id
            {
                bail!("Current local share references an invalid Split claim");
            }
            let topology_matches = previous_split_claim
                .receivers
                .iter()
                .all(|id| remaining_ids.contains(id))
                && previous_split_claim.receivers.len() == remaining_ids.len() - 1;
            // Matching receiver topology is not enough to prove that the
            // surviving local share belongs to the target K-of-N generation.
            // A two-member vault requires threshold 1; if the local share still
            // requires threshold 2, it must be regenerated even when the claim's
            // receiver list already has the right shape.
            let generation_matches_target =
                topology_matches && local_threshold == reshare_cfg.threshold;
            core_diag(&format!(
                "[CORE_DIAG] removal reshare topology actor_type={} receivers={} remaining_members={} local_threshold={} target_threshold={} topology_matches={} generation_matches_target={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                previous_split_claim.receivers.len(),
                remaining_ids.len(),
                local_threshold,
                reshare_cfg.threshold,
                topology_matches,
                generation_matches_target
            ));
            if generation_matches_target {
                core_diag(&format!(
                    "[CORE_DIAG] removal reshare skipped actor_type={} reason=current_generation_matches_target",
                    self.user_creds.device_creds.device.device_type.as_str()
                ));
                continue;
            }

            // A fresh, event-derived ID identifies this polynomial generation;
            // all devices processing the same canonical removal derive the same ID.
            let local_member = members
                .iter()
                .find(|member| member.user().device.device_id == local_device_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Local device is not in remaining members"))?;
            let generation_id =
                reshare_generation_claim_id(&previous_split_claim.id, &removal_generation_event);
            let mut split_claim = VaultMember {
                member: local_member,
                vault: vault.clone(),
            }
            .create_split_claim(transfer.pass_id.clone());
            split_claim.id = generation_id.clone();
            split_claim.dist_claim_id.id = generation_id;
            split_claim.vault_name = previous_split_claim.vault_name.clone();

            let transfer_message: EncryptedMessage =
                match serde_json::from_str(&transfer.encrypted_share) {
                    Ok(message) => message,
                    Err(error) => {
                        core_diag(
                            "[CORE_DIAG] removal reshare transfer_parse result=error class=decode",
                        );
                        return Err(error.into());
                    }
                };
            core_diag("[CORE_DIAG] removal reshare transfer_parse result=ok");
            let transferred_plain = match transfer_message
                .cipher_text()
                .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)
            {
                Ok(plain) => plain,
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare transfer_decrypt result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error);
                }
            };
            core_diag("[CORE_DIAG] removal reshare transfer_decrypt result=ok");
            let transferred_share = match UserShareDto::try_from(&transferred_plain.msg) {
                Ok(share) => share,
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare transfer_share_parse result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error.into());
                }
            };
            core_diag("[CORE_DIAG] removal reshare transfer_share_parse result=ok");
            let transferred_threshold = transferred_share
                .share_blocks
                .first()
                .map(|block| block.config.threshold)
                .unwrap_or(0);
            core_diag(&format!(
                "[CORE_DIAG] removal reshare inputs actor_type={} local_threshold={} transferred_threshold={} target_threshold={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                local_threshold,
                transferred_threshold,
                reshare_cfg.threshold
            ));

            let plain_secret = match recover_from_shares(vec![local_share, transferred_share]) {
                Ok(secret) => secret,
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare recover result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error.into());
                }
            };
            core_diag("[CORE_DIAG] removal reshare recover result=ok");
            let secure_pass = SecurePassInfo::from(PlainPassInfo {
                pass_id: transfer.pass_id.clone(),
                pass: plain_secret.text,
            });
            let re_split = match split2(secure_pass, reshare_cfg) {
                Ok(split) => split,
                Err(error) => {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare split result=error class={}",
                        core_diag_error_class(&error)
                    ));
                    return Err(error.into());
                }
            };
            core_diag(&format!(
                "[CORE_DIAG] removal reshare split result=ok shares={} threshold={}",
                re_split.shares.len(),
                reshare_cfg.threshold
            ));
            if re_split.shares.len() != members.len() {
                bail!("Removal resharing produced an invalid share count");
            }

            for (idx, receiver) in members.iter().enumerate() {
                let share_json = match re_split.shares[idx].as_json() {
                    Ok(json) => json,
                    Err(error) => {
                        core_diag(&format!(
                            "[CORE_DIAG] removal reshare share_serialize index={} result=error class={}",
                            idx,
                            core_diag_error_class(&error)
                        ));
                        return Err(error.into());
                    }
                };
                let encrypted = match self
                    .user_creds
                    .device_creds
                    .key_manager()?
                    .transport
                    .encrypt_string(
                        PlainText::from(share_json),
                        &receiver.user().device.keys.transport_pk,
                    ) {
                    Ok(encrypted) => encrypted,
                    Err(error) => {
                        core_diag(&format!(
                            "[CORE_DIAG] removal reshare share_encrypt index={} result=error class={}",
                            idx,
                            core_diag_error_class(&error)
                        ));
                        return Err(error);
                    }
                };
                let dist_id = SsDistributionId {
                    pass_id: transfer.pass_id.clone(),
                    receiver: receiver.user().device.device_id.clone(),
                };
                let wf = SsWorkflowObject::Distribution(KvLogEvent {
                    key: KvKey::from(SsWorkflowDescriptor::Distribution(dist_id)),
                    value: SecretDistributionData {
                        vault_name: update.request.sender.user_data.vault_name(),
                        claim_id: split_claim.dist_claim_id.clone(),
                        secret_message: EncryptedMessage::CipherShare { share: encrypted },
                    },
                });
                core_diag(&format!(
                    "[CORE_DIAG] removal reshare distribution_save index={} total={} result=begin",
                    idx + 1,
                    members.len()
                ));
                // Distribution is a replaceable current-generation value at this
                // descriptor. Replacing it locally lets the sender retain its own
                // newly generated share while older claim history remains in SsLog.
                self.p_obj.repo.delete(wf.obj_id()).await;
                if let Err(error) = self.p_obj.repo.save(wf).await {
                    core_diag(&format!(
                        "[CORE_DIAG] removal reshare distribution_save index={} result=error class={}",
                        idx + 1,
                        core_diag_error_class(&error)
                    ));
                    return Err(error);
                }
                core_diag(&format!(
                    "[CORE_DIAG] removal reshare distribution_save index={} total={} result=ok",
                    idx + 1,
                    members.len()
                ));
            }

            let local_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: transfer.pass_id.clone(),
                receiver: local_device_id.clone(),
            });
            // Diagnostics must never affect the removal/reshare behavior. Treat a
            // failed re-read or decode as unavailable diagnostic data.
            let (local_tail_threshold, reread_ok) =
                match self.p_obj.find_tail_event(local_desc).await {
                    Ok(Some(SsWorkflowObject::Distribution(saved_local))) => {
                        match saved_local
                            .value
                            .secret_message
                            .cipher_text()
                            .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)
                        {
                            Ok(saved_plain) => match UserShareDto::try_from(&saved_plain.msg) {
                                Ok(share) => (
                                    share
                                        .share_blocks
                                        .first()
                                        .map(|block| block.config.threshold)
                                        .unwrap_or(0),
                                    true,
                                ),
                                Err(_) => (0, false),
                            },
                            Err(_) => (0, false),
                        }
                    }
                    _ => (0, false),
                };
            core_diag(&format!(
                "[CORE_DIAG] removal reshare saved actor_type={} refreshed_members={} target_threshold={} local_tail_threshold={} reread_ok={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                members.len(),
                reshare_cfg.threshold,
                local_tail_threshold,
                reread_ok
            ));

            split_claim.receivers = members
                .iter()
                .filter_map(|member| {
                    let device_id = member.user().device.device_id.clone();
                    (device_id != local_device_id).then_some(device_id)
                })
                .collect();
            split_claim.status = SsDistributionCompositeStatus::from(split_claim.receivers.clone());
            core_diag("[CORE_DIAG] removal reshare claim_device_log_save result=begin");
            if let Err(error) = p_ss.save_claim_in_ss_device_log(split_claim.clone()).await {
                core_diag(&format!(
                    "[CORE_DIAG] removal reshare claim_device_log_save result=error class={}",
                    core_diag_error_class(&error)
                ));
                return Err(error);
            }
            core_diag("[CORE_DIAG] removal reshare claim_device_log_save result=ok");
            core_diag("[CORE_DIAG] removal reshare claim_ss_log_save result=begin");
            if let Err(error) = p_ss.save_ss_log_event(split_claim.clone()).await {
                core_diag(&format!(
                    "[CORE_DIAG] removal reshare claim_ss_log_save result=error class={}",
                    core_diag_error_class(&error)
                ));
                return Err(error);
            }
            core_diag("[CORE_DIAG] removal reshare claim_ss_log_save result=ok");
            ss_log_data
                .claims
                .insert(split_claim.id.clone(), split_claim);
        }
        Ok(())
    }

    #[cfg(any(test, feature = "test-framework"))]
    pub async fn redistribute_existing_secrets_for_test(
        &self,
        vault: &VaultData,
        join_request: &JoinClusterEvent,
    ) -> Result<()> {
        self.redistribute_existing_secrets(vault, join_request)
            .await
    }

    /// Reconcile the local sender-owned distributions after a member is accepted.
    ///
    /// Membership acceptance is a single canonical event emitted by one approver,
    /// so every other existing member must react to that event and re-share the
    /// secrets it owns. Devices that are receiver-only cannot decrypt another
    /// device's share and are skipped by `redistribute_existing_secrets`.
    pub async fn redistribute_existing_secrets_for_join(
        &self,
        join_request: &JoinClusterEvent,
    ) -> Result<()> {
        let member = self.get_member().await?;
        let vault = self
            .get_vault(member)
            .await?
            .update_membership(UserMembership::Member(UserDataMember {
                user_data: join_request.candidate.clone(),
            }));

        info!(
            actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
            accepted_member_type = %join_request.candidate.device.device_type.as_str(),
            secret_count = vault.secrets.len(),
            "join_reconcile: actor examining sender-owned distributions"
        );
        core_diag(&format!(
            "[CORE_DIAG] reconcile start actor_type={} accepted_member_type={} secret_count={}",
            self.user_creds.device_creds.device.device_type.as_str(),
            join_request.candidate.device.device_type.as_str(),
            vault.secrets.len()
        ));
        self.redistribute_existing_secrets(&vault, join_request)
            .await
    }

    async fn get_vault(&self, member: UserDataMember) -> Result<VaultData> {
        let p_vault = PersistentVault::from(self.p_obj());
        let vault = p_vault
            .get_vault(member.user().vault_name())
            .await?
            .to_data();
        Ok(vault)
    }

    async fn redistribute_existing_secrets(
        &self,
        vault: &VaultData,
        join_request: &JoinClusterEvent,
    ) -> Result<()> {
        vault.ensure_valid_member_count()?;

        let joined_device_id = join_request.candidate.device.device_id.clone();
        let local_device_id = self.user_creds.device_id().clone();
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let mut ss_log_data = self.get_ss_log_data().await?;
        let mut members = vault.members();
        members.sort_by_key(|member| member.user().device.device_id.to_string());
        let local_claim_count = ss_log_data
            .claims
            .values()
            .filter(|claim| {
                claim.distribution_type == SecretDistributionType::Split
                    && claim.sender.eq(&local_device_id)
            })
            .count();
        info!(
            actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
            member_count = members.len(),
            secret_count = vault.secrets.len(),
            local_split_claim_count = local_claim_count,
            "join_reconcile: inspected local Split claim inventory"
        );
        core_diag(&format!(
            "[CORE_DIAG] local Split inventory actor_type={} members={} secrets={} owned_claims={}",
            self.user_creds.device_creds.device.device_type.as_str(),
            members.len(),
            vault.secrets.len(),
            local_claim_count
        ));

        for pass_id in vault.secrets.iter() {
            // The current local share identifies the active claim generation. Only
            // that claim's canonical sender may create the next generation; allowing
            // every member with a share to re-split concurrently mixes polynomials.
            let source_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: local_device_id.clone(),
            });
            let Some(source_event) = self.p_obj.find_tail_event(source_desc.clone()).await? else {
                info!(
                    actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                    owns_split_claim = true,
                    local_source_share_found = false,
                    "join_reconcile: local source share missing; skipped"
                );
                core_diag(&format!(
                    "[CORE_DIAG] redistribute skipped actor_type={} reason=missing_local_source_share",
                    self.user_creds.device_creds.device.device_type.as_str()
                ));
                continue;
            };
            let SsWorkflowObject::Distribution(source_dist) = source_event else {
                continue;
            };
            let Some(split_claim) = ss_log_data
                .claims
                .get(&source_dist.value.claim_id.id)
                .cloned()
            else {
                info!(
                    actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                    owns_split_claim = false,
                    "join_reconcile: current distribution references no Split claim; skipped"
                );
                continue;
            };
            if split_claim.distribution_type != SecretDistributionType::Split
                || split_claim.dist_claim_id.pass_id != *pass_id
                || split_claim.sender != local_device_id
            {
                core_diag(&format!(
                    "[CORE_DIAG] redistribute skipped actor_type={} reason=not_canonical_split_sender",
                    self.user_creds.device_creds.device.device_type.as_str()
                ));
                continue;
            }
            let target_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: joined_device_id.clone(),
            });
            let target_distribution = self.p_obj.find_tail_event(target_desc).await?;
            let generation_id =
                reshare_generation_claim_id(&split_claim.id, &join_request.request_id);
            core_diag(&format!(
                "[CORE_DIAG] join redistribution generations actor_type={} device={} request={} source={} target={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                device_diagnostic_alias(&local_device_id),
                &sha256_hex(&join_request.request_id)[..12],
                claim_generation_alias(&split_claim.id),
                claim_generation_alias(&generation_id)
            ));
            let target_matches_join_generation = matches!(
                &target_distribution,
                Some(SsWorkflowObject::Distribution(event))
                    if event.value.claim_id.id == generation_id
            );
            let target_matches_current_generation = matches!(
                &target_distribution,
                Some(SsWorkflowObject::Distribution(event))
                    if event.value.claim_id.id == source_dist.value.claim_id.id
            );
            let claim_already_contains_joined = split_claim
                .receivers
                .iter()
                .any(|d| d.eq(&joined_device_id));
            if (target_matches_join_generation || target_matches_current_generation)
                && claim_already_contains_joined
            {
                info!(
                    actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                    target_distribution_exists = true,
                    claim_contains_target = true,
                    "join_reconcile: target already distributed; skipped"
                );
                core_diag(&format!(
                    "[CORE_DIAG] redistribute skipped actor_type={} reason=target_already_distributed current_generation_match={}",
                    self.user_creds.device_creds.device.device_type.as_str(),
                    target_matches_current_generation
                ));
                continue;
            }

            let source_plain = source_dist
                .value
                .secret_message
                .cipher_text()
                .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)?;
            let source_share = UserShareDto::try_from(&source_plain.msg)?;

            let shares_needed = source_share
                .share_blocks
                .first()
                .map(|block| block.config.threshold)
                .unwrap_or(1)
                .max(1);

            info!(
                actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                owns_split_claim = true,
                local_source_share_found = true,
                shares_needed,
                "join_reconcile: local owner claim and source share found"
            );
            core_diag(&format!(
                "[CORE_DIAG] local share lookup actor_type={} owns_split_claim=true local_source_share_found=true shares_needed={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                shares_needed
            ));

            let mut shares_for_recovery: Vec<UserShareDto> = vec![source_share.clone()];
            let mut collected_share_ids: HashSet<usize> = HashSet::from([source_share.share_id]);

            let mut candidate_devices: Vec<_> = members
                .iter()
                .map(|member| member.user().device.device_id.clone())
                .collect();
            candidate_devices.sort_by_key(|device_id| device_id.to_string());
            candidate_devices.dedup();

            for receiver in candidate_devices {
                if shares_for_recovery.len() >= shares_needed {
                    break;
                }
                if receiver.eq(&local_device_id) {
                    continue;
                }

                let dist_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: pass_id.clone(),
                    receiver,
                });
                let Some(dist_event) = self.p_obj.find_tail_event(dist_desc).await? else {
                    continue;
                };

                let SsWorkflowObject::Distribution(dist) = dist_event else {
                    continue;
                };

                let decrypted = match dist
                    .value
                    .secret_message
                    .cipher_text()
                    .decrypt(&self.user_creds.device_creds.secret_box.transport.sk)
                {
                    Ok(plain) => plain,
                    Err(_) => continue,
                };

                let share = match UserShareDto::try_from(&decrypted.msg) {
                    Ok(share) => share,
                    Err(_) => continue,
                };

                if collected_share_ids.insert(share.share_id) {
                    shares_for_recovery.push(share);
                }
            }

            if shares_for_recovery.len() < shares_needed {
                info!(
                    actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                    shares_needed,
                    shares_found = shares_for_recovery.len(),
                    "join_reconcile: insufficient shares to reconstruct secret; skipped"
                );
                core_diag(&format!(
                    "[CORE_DIAG] redistribute skipped actor_type={} reason=insufficient_shares shares_needed={} shares_found={}",
                    self.user_creds.device_creds.device.device_type.as_str(),
                    shares_needed,
                    shares_for_recovery.len()
                ));
                continue;
            }

            info!(
                actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                shares_needed,
                "Redistributing sender-owned secret for vault member"
            );

            let plain_secret = recover_from_shares(shares_for_recovery)?;
            let secure_pass = SecurePassInfo::from(PlainPassInfo {
                pass_id: pass_id.clone(),
                pass: plain_secret.text,
            });
            let re_split = split2(secure_pass, vault.sss_cfg())?;

            if re_split.shares.len() != members.len() {
                bail!("Invalid state: shares count does not match vault members count");
            }

            // A re-split is a new Shamir generation. Give it a fresh claim identity
            // so the server initializes receiver statuses for this generation.
            let local_member = members
                .iter()
                .find(|member| member.user().device.device_id == local_device_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Local device is not in vault members"))?;
            let mut split_claim = VaultMember {
                member: local_member,
                vault: vault.clone(),
            }
            .create_split_claim(pass_id.clone());
            split_claim.id = generation_id.clone();
            split_claim.dist_claim_id.id = generation_id;
            split_claim.vault_name = source_dist.value.vault_name.clone();

            for (idx, receiver) in members.iter().enumerate() {
                let receiver_pk = &receiver.user().device.keys.transport_pk;
                let share_json = re_split.shares[idx].as_json()?;
                let encrypted = self
                    .user_creds
                    .device_creds
                    .key_manager()?
                    .transport
                    .encrypt_string(PlainText::from(share_json), receiver_pk)?;

                let dist_id = SsDistributionId {
                    pass_id: pass_id.clone(),
                    receiver: receiver.user().device.device_id.clone(),
                };

                let wf = SsWorkflowObject::Distribution(KvLogEvent {
                    key: KvKey::from(SsWorkflowDescriptor::Distribution(dist_id)),
                    value: SecretDistributionData {
                        vault_name: source_dist.value.vault_name.clone(),
                        claim_id: split_claim.dist_claim_id.clone(),
                        secret_message: EncryptedMessage::CipherShare { share: encrypted },
                    },
                });
                // Delete any stale distribution before saving the new one so all members'
                // shares always come from the same polynomial after redistribution.
                self.p_obj.repo.delete(wf.obj_id()).await;
                self.p_obj.repo.save(wf).await?;
            }

            // create_split_claim populated the exact current receiver set as Pending.
            // Persist the updated claim in the device log too so the next sync
            // sends the new receiver list to the server.
            p_ss.save_claim_in_ss_device_log(split_claim.clone())
                .await?;
            p_ss.save_ss_log_event(split_claim.clone()).await?;
            ss_log_data
                .claims
                .insert(split_claim.id.clone(), split_claim);

            info!(
                actor_type = %self.user_creds.device_creds.device.device_type.as_str(),
                member_count = members.len(),
                "join_reconcile: refreshed Split claim and distributions saved locally"
            );
            core_diag(&format!(
                "[CORE_DIAG] redistribution saved actor_type={} refreshed_members={}",
                self.user_creds.device_creds.device.device_type.as_str(),
                members.len()
            ));
        }

        Ok(())
    }

    async fn get_vault_log_event(&self, member: &UserDataMember) -> Result<Option<VaultLogObject>> {
        let p_vault = PersistentVault::from(self.p_obj());
        let maybe_vault_log_event = {
            let vault_name = member.user_data.vault_name();
            p_vault.vault_log(vault_name).await?
        };
        Ok(maybe_vault_log_event)
    }

    async fn get_member(&self) -> Result<UserDataMember> {
        let p_vault = PersistentVault::from(self.p_obj());
        let vault_status = p_vault.find(self.user_creds.user()).await?;

        let VaultStatus::Member(member) = vault_status else {
            bail!("Not a vault member");
        };

        Ok(member)
    }

    async fn get_ss_log_data(&self) -> Result<SsLogData> {
        let ss_log_data = {
            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
            p_ss.get_ss_log_obj(self.user_creds.vault_name.clone())
                .await?
        };
        Ok(ss_log_data)
    }

    /// When the receiver accepts, re-encrypts its share for the claim sender and saves
    /// the recovery workflow locally. Sync will push it to the server so the sender can pull and decrypt.
    async fn handle_recover(&self, vault: VaultData, claim: SsClaim) -> Result<()> {
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

        //get distributions
        for claim_db_id in claim.recovery_db_ids() {
            //get distribution id
            let local_device = self.user_creds.device_id();

            let claim_sender_device = claim_db_id.sender.clone();
            let claim_receiver = claim_db_id.distribution_id.receiver.clone();
            if !claim_receiver.eq(local_device) {
                debug!("Ignore any claims for other devices");
                continue;
            }

            let ss_dist_obj = p_ss
                .get_ss_distribution_event_by_id(claim_db_id.distribution_id.clone())
                .await?;

            let SsWorkflowObject::Distribution(dist_event) = ss_dist_obj else {
                let msg_err = "Ss distribution object not found.";
                let msg_info = "Verify the Distribution event is not messed up (sender and receiver not swapped)";
                bail!("{} {}", msg_err, msg_info);
            };

            let KvLogEvent { value: share, .. } = dist_event;

            let maybe_claim_sender = vault.find_user(&claim_sender_device);
            match maybe_claim_sender {
                None => {
                    bail!("Claim sender is not a member of the vault")
                }
                Some(claim_sender) => {
                    match claim_sender {
                        UserMembership::Outsider(_) => {
                            bail!("Claim sender is not a member of the vault")
                        }
                        UserMembership::Member(_) => {
                            // re encrypt message
                            let msg_receiver = &claim_sender.user_data().device.keys.transport_pk;
                            let msg = self
                                .user_creds
                                .device_creds
                                .secret_box
                                .re_encrypt(share.secret_message.clone(), msg_receiver)?;

                            //compare with claim dist id, if match then create a claim
                            let key =
                                KvKey::from(SsWorkflowDescriptor::Recovery(claim_db_id.clone()));

                            let new_wf_event = SsWorkflowObject::Recovery(KvLogEvent {
                                key,
                                value: SecretDistributionData {
                                    vault_name: self.user_creds.vault_name.clone(),
                                    claim_id: claim_db_id.claim_id,
                                    secret_message: msg,
                                },
                            });

                            p_ss.p_obj.repo.save(new_wf_event).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.p_obj.clone()
    }
}

/// Only the approver owns the immediate 3→2 resharing operation. The
/// initiator's request transfers are its old K=2 input shares; using them to
/// independently generate a second random K=1 polynomial would leave its
/// local distributions inconsistent with the approver's result.
fn should_retry_removal_reshare(
    local_device_id: &DeviceId,
    approver_device_id: &DeviceId,
    has_request_share_transfers: bool,
) -> bool {
    approver_device_id == local_device_id && has_request_share_transfers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::utils::Id48bit;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo};
    use crate::node::common::model::secret::{
        SsClaimId, SsDistributionCompositeStatus, SsDistributionId, SsRecoveryId,
    };
    use crate::node::common::model::user::common::{UserDataOutsider, UserDataOutsiderStatus};
    use crate::node::common::model::vault::vault_data::VaultData;
    use crate::node::db::descriptors::vault_descriptor::VaultStatusDescriptor;
    use crate::node::db::descriptors::vault_descriptor::{VaultDescriptor, VaultLogDescriptor};
    use crate::node::db::events::object_id::ArtifactId;
    use crate::node::db::events::shared_secret_event::SsWorkflowObject;
    use crate::node::db::events::vault::vault_event::VaultObject;
    use crate::node::db::events::vault::vault_log_event::RemoveMemberDecision;
    use crate::node::db::events::vault::vault_status::VaultStatusObject;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use crate::node::db::repo::generic_db::{DeleteCommand, SaveCommand};
    use crate::secret::MetaDistributor;
    use anyhow::Result;

    #[test]
    fn removal_reshare_retry_is_owned_only_by_approver() {
        let initiator = DeviceId(crate::crypto::utils::U64IdUrlEnc::from(
            "initiator".to_string(),
        ));
        let approver = DeviceId(crate::crypto::utils::U64IdUrlEnc::from(
            "approver".to_string(),
        ));

        assert!(!should_retry_removal_reshare(&initiator, &approver, true));
        assert!(should_retry_removal_reshare(&approver, &approver, true));
        assert!(!should_retry_removal_reshare(&approver, &approver, false));
    }

    async fn prepare_single_device_secret() -> Result<(
        FixtureRegistry<EmptyState>,
        MetaOrchestrator<InMemKvLogEventRepo>,
        VaultData,
        MetaPasswordId,
    )> {
        let registry = FixtureRegistry::empty();
        let client_user_creds = registry.state.user_creds.client.clone();
        let client_member = registry
            .state
            .vault_data
            .client_membership
            .user_data_member();
        let single_member_vault = VaultData::from(client_member.clone());

        let vault_member = VaultMember {
            member: client_member,
            vault: single_member_vault.clone(),
        };

        let pass_info = PlainPassInfo::new("late_join_secret".to_string(), "2bee|~".to_string());
        let secure_pass = SecurePassInfo::from(pass_info);
        let pass_id = secure_pass.pass_id.clone();

        let distributor = MetaDistributor {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: Arc::new(client_user_creds.clone()),
            vault_member: vault_member.clone(),
        };
        distributor
            .distribute(vault_member.clone(), secure_pass)
            .await?;

        // In this unit test we do not run server sync, so seed ss_log explicitly.
        let p_ss = PersistentSharedSecret::from(registry.state.p_obj.client.clone());
        let seeded_claim = p_ss
            .find_ss_device_log_tail_event(
                registry
                    .state
                    .vault_data
                    .client_membership
                    .user_data_member()
                    .user()
                    .device
                    .device_id
                    .clone(),
            )
            .await?
            .expect("MetaDistributor must persist its exact Split claim")
            .to_distribution_request();
        p_ss.save_ss_log_event(seeded_claim).await?;

        let orchestrator = MetaOrchestrator {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: client_user_creds,
        };

        Ok((registry, orchestrator, single_member_vault, pass_id))
    }

    #[tokio::test]
    async fn test_redistribute_existing_secret_on_join_accept() -> Result<()> {
        let (registry, orchestrator, single_member_vault, pass_id) =
            prepare_single_device_secret().await?;

        let joined_member = registry.state.vault_data.vd_membership.user_data_member();
        let updated_vault = single_member_vault
            .update_membership(UserMembership::Member(joined_member.clone()))
            .add_secret(pass_id.clone());

        let join_request = JoinClusterEvent {
            candidate: joined_member.user().clone(),
            request_id: "redistribute-test".into(),
        };

        orchestrator
            .redistribute_existing_secrets(&updated_vault, &join_request)
            .await?;

        let target_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass_id.clone(),
            receiver: joined_member.user().device.device_id.clone(),
        });
        let target_event = orchestrator.p_obj.find_tail_event(target_desc).await?;
        assert!(
            target_event.is_some(),
            "Distribution for newly joined member must be created"
        );

        let Some(SsWorkflowObject::Distribution(dist)) = target_event else {
            panic!("Expected distribution workflow object");
        };

        let joined_sk = &registry
            .state
            .user_creds
            .vd
            .device_creds
            .secret_box
            .transport
            .sk;
        let decrypted = dist.value.secret_message.cipher_text().decrypt(joined_sk)?;
        let share_json = String::try_from(&decrypted.msg)?;
        assert!(
            share_json.contains("share_id"),
            "Redistributed payload must contain a valid share"
        );

        let p_ss = PersistentSharedSecret::from(orchestrator.p_obj.clone());
        let device_log_event = p_ss
            .find_ss_device_log_tail_event(
                registry
                    .state
                    .vault_data
                    .client_membership
                    .user_data_member()
                    .user()
                    .device
                    .device_id
                    .clone(),
            )
            .await?;
        let Some(device_log_event) = device_log_event else {
            panic!("Expected updated device log claim");
        };
        let updated_claim = device_log_event.to_distribution_request();
        assert!(
            updated_claim
                .receivers
                .iter()
                .any(|device_id| device_id == &joined_member.user().device.device_id),
            "Updated claim must include the newly joined receiver"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_redistribute_existing_secret_is_idempotent() -> Result<()> {
        let (registry, orchestrator, single_member_vault, pass_id) =
            prepare_single_device_secret().await?;

        let joined_member = registry.state.vault_data.vd_membership.user_data_member();
        let updated_vault = single_member_vault
            .update_membership(UserMembership::Member(joined_member.clone()))
            .add_secret(pass_id.clone());
        let join_request = JoinClusterEvent {
            candidate: joined_member.user().clone(),
            request_id: "redistribute-idempotent-test".into(),
        };

        orchestrator
            .redistribute_existing_secrets(&updated_vault, &join_request)
            .await?;
        let db_len_after_first = orchestrator.p_obj.repo.get_db().await.len();

        orchestrator
            .redistribute_existing_secrets(&updated_vault, &join_request)
            .await?;
        let db_len_after_second = orchestrator.p_obj.repo.get_db().await.len();

        assert_eq!(
            db_len_after_first, db_len_after_second,
            "Second redistribution run must not create additional events"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_removal_resharing_replaces_initiator_share_with_two_device_replica() -> Result<()>
    {
        let registry = FixtureRegistry::empty();
        let client_creds = registry.state.user_creds.client.clone();
        let client_member = registry
            .state
            .vault_data
            .client_membership
            .user_data_member();
        let approver_member = registry
            .state
            .vault_data
            .client_b_membership
            .user_data_member();
        let target_member = registry.state.vault_data.vd_membership.user_data_member();
        let three_device_vault = registry.state.vault_data.full_membership.clone();
        let vault_member = VaultMember {
            member: client_member.clone(),
            vault: three_device_vault.clone(),
        };
        let expected_secret = "removal-resharing-secret";
        let secure_pass = SecurePassInfo::from(PlainPassInfo::new(
            "removal-resharing".to_string(),
            expected_secret.to_string(),
        ));
        let pass_id = secure_pass.pass_id.clone();

        MetaDistributor {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: Arc::new(client_creds.clone()),
            vault_member: vault_member.clone(),
        }
        .distribute(vault_member.clone(), secure_pass)
        .await?;

        let p_ss = PersistentSharedSecret::from(registry.state.p_obj.client.clone());
        let split_claim = p_ss
            .find_ss_device_log_tail_event(client_member.user().device.device_id.clone())
            .await?
            .expect("MetaDistributor must persist its exact Split claim")
            .to_distribution_request();
        p_ss.save_ss_log_event(split_claim).await?;

        let initiator_distribution = registry
            .state
            .p_obj
            .client
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: client_member.user().device.device_id.clone(),
            }))
            .await?
            .expect("initiator distribution must exist")
            .to_distribution_data()?;
        let initiator_plain = initiator_distribution
            .secret_message
            .cipher_text()
            .decrypt(
                &registry
                    .state
                    .user_creds
                    .client
                    .device_creds
                    .secret_box
                    .transport
                    .sk,
            )?;
        let initiator_share = UserShareDto::try_from(&initiator_plain.msg)?;
        let encrypted_for_approver = registry
            .state
            .user_creds
            .client
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(
                PlainText::from(initiator_share.as_json()?),
                &approver_member.user().device.keys.transport_pk,
            )?;
        let transfer = RemoveMemberShareTransfer {
            pass_id: pass_id.clone(),
            receiver: approver_member.user().device.device_id.clone(),
            claim_id: Some(initiator_distribution.claim_id.id.clone()),
            encrypted_share: serde_json::to_string(&EncryptedMessage::CipherShare {
                share: encrypted_for_approver,
            })?,
        };
        let declined_target = UserMembership::Outsider(UserDataOutsider {
            user_data: target_member.user().clone(),
            status: UserDataOutsiderStatus::Declined,
        });
        let request = RemoveMemberRequestEvent {
            sender: client_member.clone(),
            target: target_member.clone(),
            share_transfers: vec![transfer.clone()],
        };
        let update = RemoveMemberEvent {
            request: request.clone(),
            sender: approver_member.clone(),
            target: target_member.clone(),
            new_status: declined_target.clone(),
            decision: RemoveMemberDecision::Accepted,
            share_transfers: vec![],
        };
        let two_device_vault = three_device_vault
            .update_membership(declined_target)
            .add_secret(pass_id.clone());
        let orchestrator = MetaOrchestrator {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: client_creds.clone(),
        };
        let approver_orchestrator = MetaOrchestrator {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: registry.state.user_creds.client_b.clone(),
        };
        let approver_creds = registry.state.user_creds.client_b.clone();

        orchestrator
            .p_obj
            .repo
            .save(VaultObject(KvLogEvent {
                key: KvKey::from(VaultDescriptor::from(client_creds.vault_name.clone())),
                value: two_device_vault.clone(),
            }))
            .await?;
        orchestrator
            .p_obj
            .repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(client_member.clone()),
                ArtifactId::from(VaultStatusDescriptor::from(client_creds.user().user_id())),
            ))
            .await?;
        orchestrator
            .p_obj
            .repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(approver_member.clone()),
                ArtifactId::from(VaultStatusDescriptor::from(approver_creds.user().user_id())),
            ))
            .await?;
        let update = update.clone();
        let vault_events =
            crate::node::db::events::vault::vault_log_event::VaultActionEvents::default()
                .request(VaultActionRequestEvent::RemoveMember(request))
                .apply(VaultActionUpdateEvent::RemoveMember(update.clone()));
        orchestrator
            .p_obj
            .repo
            .save(VaultLogObject(KvLogEvent {
                key: KvKey::from(VaultLogDescriptor::from(client_creds.vault_name.clone())),
                value: vault_events,
            }))
            .await?;

        // Model the immediate approver path, then the initiator's later
        // getAppState retry. The retry must not create a second random split.
        approver_orchestrator
            .reshare_after_removal(&two_device_vault, &update, vec![transfer], false)
            .await?;

        let before_approver_retry = approver_orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: approver_creds.device_id().clone(),
            }))
            .await?
            .expect("approver distribution must exist")
            .to_distribution_data()?;
        let before_approver_retry = serde_json::to_string(&before_approver_retry)?;
        approver_orchestrator
            .finalize_pending_removal_resharing()
            .await?;
        let after_approver_retry = approver_orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: approver_creds.device_id().clone(),
            }))
            .await?
            .expect("approver distribution must remain present")
            .to_distribution_data()?;
        assert_eq!(
            before_approver_retry,
            serde_json::to_string(&after_approver_retry)?,
            "approver retry after successful immediate resharing must be idempotent"
        );

        let before_initiator_retry = orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: client_creds.device_id().clone(),
            }))
            .await?
            .expect("initiator distribution must exist")
            .to_distribution_data()?;
        let before_initiator_retry = serde_json::to_string(&before_initiator_retry)?;
        orchestrator.finalize_pending_removal_resharing().await?;
        let after_initiator_retry = orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: client_creds.device_id().clone(),
            }))
            .await?
            .expect("initiator distribution must exist")
            .to_distribution_data()?;
        assert_eq!(
            before_initiator_retry,
            serde_json::to_string(&after_initiator_retry)?,
            "initiator retry must not replace the approver's share generation"
        );

        let local_distribution = orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: client_creds.device_id().clone(),
            }))
            .await?
            .expect("initiator distribution must exist")
            .to_distribution_data()?;
        let local_plain = local_distribution
            .secret_message
            .cipher_text()
            .decrypt(&client_creds.device_creds.secret_box.transport.sk)?;
        let local_share = UserShareDto::try_from(&local_plain.msg)?;

        assert_eq!(local_share.share_blocks[0].config.threshold, 1);
        assert_eq!(
            recover_from_shares(vec![local_share])?.text,
            expected_secret
        );

        // Exercise the same secret through the next 2→3→2 membership churn.
        // The member that owns the post-removal claim joins the third device,
        // then the other remaining member approves its removal.
        let rejoined_vault = two_device_vault
            .clone()
            .update_membership(UserMembership::Member(target_member.clone()));
        let join_request = JoinClusterEvent {
            candidate: target_member.user().clone(),
            request_id: "removal-reshare-generation-regression".into(),
        };
        approver_orchestrator
            .redistribute_existing_secrets(&rejoined_vault, &join_request)
            .await?;

        let initiator_share_after_join = orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: client_creds.device_id().clone(),
            }))
            .await?
            .expect("initiator must receive the joined K=2 share")
            .to_distribution_data()?;
        let second_transfer_claim_id = initiator_share_after_join.claim_id.id.clone();
        let initiator_plain_after_join = initiator_share_after_join
            .secret_message
            .cipher_text()
            .decrypt(&client_creds.device_creds.secret_box.transport.sk)?;
        let initiator_share_after_join = UserShareDto::try_from(&initiator_plain_after_join.msg)?;
        assert_eq!(
            initiator_share_after_join.share_blocks[0].config.threshold,
            2
        );
        let approver_share_after_join = orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: approver_creds.device_id().clone(),
            }))
            .await?
            .expect("approver must receive the joined K=2 share")
            .to_distribution_data()?;
        let approver_plain_after_join = approver_share_after_join
            .secret_message
            .cipher_text()
            .decrypt(&approver_creds.device_creds.secret_box.transport.sk)?;
        let approver_share_after_join = UserShareDto::try_from(&approver_plain_after_join.msg)?;
        assert_eq!(
            approver_share_after_join.share_blocks[0].config.threshold,
            2
        );
        let post_join_recovery = recover_from_shares(vec![
            initiator_share_after_join.clone(),
            approver_share_after_join.clone(),
        ]);
        assert!(
            post_join_recovery.is_ok(),
            "join resharing must leave compatible K=2 shares before next removal: {post_join_recovery:?}"
        );

        let encrypted_for_approver = client_creds
            .device_creds
            .key_manager()?
            .transport
            .encrypt_string(
                PlainText::from(initiator_share_after_join.as_json()?),
                &registry
                    .state
                    .vault_data
                    .client_b_membership
                    .user_data_member()
                    .user()
                    .device
                    .keys
                    .transport_pk,
            )?;
        let second_transfer = RemoveMemberShareTransfer {
            pass_id: pass_id.clone(),
            receiver: registry
                .state
                .vault_data
                .client_b_membership
                .user_data_member()
                .user()
                .device
                .device_id
                .clone(),
            claim_id: Some(second_transfer_claim_id),
            encrypted_share: serde_json::to_string(&EncryptedMessage::CipherShare {
                share: encrypted_for_approver,
            })?,
        };
        let transfer_message: EncryptedMessage =
            serde_json::from_str(&second_transfer.encrypted_share)?;
        let transferred_plain = transfer_message
            .cipher_text()
            .decrypt(&approver_creds.device_creds.secret_box.transport.sk)?;
        let transferred_share = UserShareDto::try_from(&transferred_plain.msg)?;
        assert_eq!(
            recover_from_shares(vec![approver_share_after_join.clone(), transferred_share])?.text,
            expected_secret,
            "the exact removal transfer and approver-local share must match before retry"
        );
        let second_request = RemoveMemberRequestEvent {
            sender: client_member.clone(),
            target: target_member.clone(),
            share_transfers: vec![second_transfer.clone()],
        };
        let second_update = RemoveMemberEvent {
            request: second_request,
            sender: registry
                .state
                .vault_data
                .client_b_membership
                .user_data_member(),
            target: target_member.clone(),
            new_status: UserMembership::Outsider(UserDataOutsider {
                user_data: target_member.user().clone(),
                status: UserDataOutsiderStatus::Declined,
            }),
            decision: RemoveMemberDecision::Accepted,
            share_transfers: vec![],
        };
        let final_two_device_vault =
            rejoined_vault.update_membership(second_update.new_status.clone());
        approver_orchestrator
            .p_obj
            .repo
            .save(VaultObject(KvLogEvent {
                key: KvKey::from(VaultDescriptor::from(approver_creds.vault_name.clone())),
                value: final_two_device_vault.clone(),
            }))
            .await?;
        let second_vault_events =
            crate::node::db::events::vault::vault_log_event::VaultActionEvents::default()
                .request(VaultActionRequestEvent::RemoveMember(
                    second_update.request.clone(),
                ))
                .apply(VaultActionUpdateEvent::RemoveMember(second_update.clone()));
        let second_log = VaultLogObject(KvLogEvent {
            key: KvKey::from(VaultLogDescriptor::from(approver_creds.vault_name.clone())),
            value: second_vault_events,
        });
        approver_orchestrator
            .p_obj
            .repo
            .delete(second_log.obj_id())
            .await;
        approver_orchestrator.p_obj.repo.save(second_log).await?;
        let persisted_removal_log = approver_orchestrator
            .get_vault_log_event(&second_update.sender)
            .await?
            .expect("second removal update must be persisted");
        let persisted_latest_transfer = persisted_removal_log
            .0
            .value
            .updates
            .into_iter()
            .filter_map(|event| match event {
                VaultActionUpdateEvent::RemoveMember(update)
                    if update.sender.user_data.device.device_id
                        == approver_creds.device_id().clone() =>
                {
                    update.request.share_transfers.last().cloned()
                }
                _ => None,
            })
            .last()
            .expect("approver's removal update must retain its request transfer");
        let persisted_transfer_message: EncryptedMessage =
            serde_json::from_str(&persisted_latest_transfer.encrypted_share)?;
        let persisted_transfer_plain = persisted_transfer_message
            .cipher_text()
            .decrypt(&approver_creds.device_creds.secret_box.transport.sk)?;
        let persisted_transfer_share = UserShareDto::try_from(&persisted_transfer_plain.msg)?;
        assert!(
            recover_from_shares(vec![
                approver_share_after_join.clone(),
                persisted_transfer_share
            ])
            .is_ok(),
            "persisted retry transfer must match the current K=2 share generation"
        );
        // Model a process interruption before immediate reshare persistence:
        // after restart only the approver's finalizer may complete it.
        approver_orchestrator
            .finalize_pending_removal_resharing()
            .await?;

        for member in [&client_member, &second_update.sender] {
            let distribution = orchestrator
                .p_obj
                .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                    pass_id: pass_id.clone(),
                    receiver: member.user().device.device_id.clone(),
                }))
                .await?
                .expect("both remaining members must receive the new K=1 generation")
                .to_distribution_data()?;
            let creds = if member.user().device.device_id == client_creds.device_id().clone() {
                &client_creds
            } else {
                &approver_creds
            };
            let plain = distribution
                .secret_message
                .cipher_text()
                .decrypt(&creds.device_creds.secret_box.transport.sk)?;
            let share = UserShareDto::try_from(&plain.msg)?;
            assert_eq!(share.share_blocks[0].config.threshold, 1);
            assert_eq!(recover_from_shares(vec![share])?.text, expected_secret);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_two_to_one_removal_reassigns_foreign_secret_before_rejoin() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let survivor_creds = registry.state.user_creds.client.clone();
        let survivor = registry
            .state
            .vault_data
            .client_membership
            .user_data_member();
        let removed_member = registry.state.vault_data.vd_membership.user_data_member();
        let two_device_vault = VaultData::from(survivor.clone())
            .update_membership(UserMembership::Member(removed_member.clone()));
        let vault_member = VaultMember {
            member: survivor.clone(),
            vault: two_device_vault.clone(),
        };
        let expected_secret = "foreign-owner-survives-two-to-one";
        let secure_pass = SecurePassInfo::from(PlainPassInfo::new(
            "foreign-owner-two-to-one".to_string(),
            expected_secret.to_string(),
        ));
        let pass_id = secure_pass.pass_id.clone();

        MetaDistributor {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: Arc::new(survivor_creds.clone()),
            vault_member: vault_member.clone(),
        }
        .distribute(vault_member, secure_pass)
        .await?;

        let p_ss = PersistentSharedSecret::from(registry.state.p_obj.client.clone());
        let local_device_claim = p_ss
            .find_ss_device_log_tail_event(survivor_creds.device_id().clone())
            .await?
            .expect("local split claim must exist")
            .to_distribution_request();
        let mut foreign_sender_claim = local_device_claim.clone();
        foreign_sender_claim.sender = removed_member.user().device.device_id.clone();
        foreign_sender_claim.receivers = vec![survivor_creds.device_id().clone()];
        foreign_sender_claim.status =
            SsDistributionCompositeStatus::from(foreign_sender_claim.receivers.clone());
        p_ss.save_ss_log_event(foreign_sender_claim).await?;

        let persisted_vault = two_device_vault.add_secret(pass_id.clone());
        registry
            .state
            .p_obj
            .client
            .repo
            .save(VaultObject(KvLogEvent {
                key: KvKey::from(VaultDescriptor::from(survivor_creds.vault_name.clone())),
                value: persisted_vault.clone(),
            }))
            .await?;
        registry
            .state
            .p_obj
            .client
            .repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(survivor.clone()),
                ArtifactId::from(VaultStatusDescriptor::from(survivor_creds.user().user_id())),
            ))
            .await?;

        let orchestrator = MetaOrchestrator {
            p_obj: registry.state.p_obj.client.clone(),
            user_creds: survivor_creds.clone(),
        };
        orchestrator
            .request_remove_member(removed_member.clone())
            .await?;

        let post_removal_claim = orchestrator
            .get_ss_log_data()
            .await?
            .claims
            .get(&local_device_claim.id)
            .cloned()
            .expect("split claim must remain after removal");
        assert_eq!(
            post_removal_claim.sender,
            survivor_creds.device_id().clone(),
            "the sole remaining device must become the canonical sender"
        );
        assert!(
            post_removal_claim.receivers.is_empty(),
            "a single-device split claim must not retain removed receivers"
        );
        assert!(
            post_removal_claim.status.statuses.is_empty(),
            "a single-device split claim must not retain receiver statuses"
        );

        let rejoin_request = JoinClusterEvent {
            candidate: removed_member.user().clone(),
            request_id: "rejoin-test".into(),
        };
        let rejoined_vault = VaultData::from(survivor)
            .update_membership(UserMembership::Member(removed_member.clone()))
            .add_secret(pass_id.clone());
        orchestrator
            .redistribute_existing_secrets(&rejoined_vault, &rejoin_request)
            .await?;

        let rejoined_distribution = orchestrator
            .p_obj
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id,
                receiver: removed_member.user().device.device_id.clone(),
            }))
            .await?
            .expect("rejoined device must receive the foreign-created secret")
            .to_distribution_data()?;
        let rejoined_plain = rejoined_distribution.secret_message.cipher_text().decrypt(
            &registry
                .state
                .user_creds
                .vd
                .device_creds
                .secret_box
                .transport
                .sk,
        )?;
        let rejoined_share = UserShareDto::try_from(&rejoined_plain.msg)?;
        assert_eq!(
            recover_from_shares(vec![rejoined_share])?.text,
            expected_secret
        );
        Ok(())
    }

    #[tokio::test]
    async fn test20_accept_recover_ignores_foreign_and_unknown_claims() -> Result<()> {
        let registry = FixtureRegistry::base().await?;
        let client_user_creds = registry.state.empty.user_creds.client.clone();
        let orchestrator = MetaOrchestrator {
            p_obj: registry.state.spec.client.p_obj.clone(),
            user_creds: client_user_creds,
        };
        let client_user = orchestrator.user_creds.user();
        let client_member = UserDataMember::from(client_user.clone());
        orchestrator
            .p_obj
            .repo
            .save(VaultObject::sign_up(
                client_user.vault_name(),
                client_member.clone(),
            ))
            .await?;
        orchestrator
            .p_obj
            .repo
            .save(VaultStatusObject::new(
                VaultStatus::Member(client_member),
                ArtifactId::from(VaultStatusDescriptor::from(client_user.user_id())),
            ))
            .await?;
        let foreign_receiver = registry
            .state
            .empty
            .vault_data
            .vd_membership
            .user_data_member()
            .user()
            .device
            .device_id
            .clone();
        let pass_info = PlainPassInfo::new("test20_foreign".to_string(), "foreign|~".to_string());
        let pass_id = SecurePassInfo::from(pass_info).pass_id;
        let claim_id = ClaimId::from(Id48bit::generate());
        let claim = SsClaim {
            id: claim_id.clone(),
            dist_claim_id: SsClaimId {
                id: claim_id.clone(),
                pass_id: pass_id.clone(),
            },
            vault_name: orchestrator.user_creds.vault_name.clone(),
            sender: orchestrator.user_creds.device_id().clone(),
            distribution_type: SecretDistributionType::Recover,
            receivers: vec![foreign_receiver.clone()],
            status: SsDistributionCompositeStatus::from(vec![foreign_receiver.clone()]),
            client_status: None,
        };

        let p_ss = PersistentSharedSecret::from(orchestrator.p_obj.clone());
        p_ss.save_ss_log_event(claim.clone()).await?;
        orchestrator.accept_recover(claim_id.clone()).await?;

        let recovery_descriptor = SsWorkflowDescriptor::Recovery(SsRecoveryId {
            claim_id: claim.dist_claim_id.clone(),
            sender: claim.sender.clone(),
            distribution_id: SsDistributionId {
                pass_id,
                receiver: foreign_receiver,
            },
        });
        assert!(
            orchestrator
                .p_obj
                .find_tail_event(recovery_descriptor)
                .await?
                .is_none(),
            "a device that is not listed in receivers must not create a recovery response"
        );

        let unknown_claim = ClaimId::from(Id48bit::generate());
        orchestrator.accept_recover(unknown_claim).await?;
        Ok(())
    }
}
