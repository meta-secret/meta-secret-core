use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::meta_pass::{PlainPassInfo, SecurePassInfo};
use crate::node::common::model::secret::{
    ClaimId, SecretDistributionData, SecretDistributionType, SsClaim, SsDeclineData,
    SsDistributionId, SsDistributionStatus, SsLogData,
};
use crate::node::common::model::user::common::{UserDataMember, UserMembership};
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::actions::sign_up::join::{JoinAction, JoinActionUpdate};
use crate::node::db::descriptors::shared_secret_descriptor::SsDeviceLogDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::events::generic_log_event::ObjIdExtractor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::{SsDeviceLogObject, SsWorkflowObject};
use crate::node::db::events::vault::vault_log_event::{
    JoinClusterEvent, VaultActionRequestEvent, VaultLogObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::recover_from_shares;
use crate::secret::split2;
use crate::secret::shared_secret::{PlainText, UserShareDto};
use anyhow::bail;
use anyhow::Result;
use log::debug;
use std::collections::HashSet;
use std::sync::Arc;

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
        println!("🦀 Orchestrator: updated_claim status: {:?}", updated_claim.status);

        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        p_ss.save_ss_log_event(updated_claim).await?;

        for recovery_db_id in claim.recovery_db_ids() {
            if recovery_db_id.distribution_id.receiver.eq(&local_device_id) {
                let decline_data = SsDeclineData {
                    vault_name: claim.vault_name.clone(),
                    claim_id: claim.id.clone(),
                    receiver_id: local_device_id.clone(),
                };
                let key =
                    KvKey::from(SsWorkflowDescriptor::Decline(recovery_db_id.clone()));
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
                    if join_request.eq(&db_join_request) {
                        let join_action = JoinAction {
                            p_obj: self.p_obj.clone(),
                            member: VaultMember {
                                member: member.clone(),
                                vault: vault.clone(),
                            },
                        };

                        join_action.update(db_join_request, upd.clone()).await?;

                        if let JoinActionUpdate::Accept = upd {
                            let redistribution_vault = vault.clone().update_membership(
                                UserMembership::Member(UserDataMember {
                                    user_data: join_request.candidate.clone(),
                                }),
                            );
                            self.redistribute_existing_secrets(
                                &redistribution_vault,
                                &join_request,
                            )
                                .await?;
                        }
                    }
                }
                VaultActionRequestEvent::AddMetaPass(_) => {
                    //Ignore server side events (no need approval)
                }
            }
        }

        Ok(())
    }

    #[cfg(any(test, feature = "test-framework"))]
    pub async fn redistribute_existing_secrets_for_test(
        &self,
        vault: &VaultData,
        join_request: &JoinClusterEvent,
    ) -> Result<()> {
        self.redistribute_existing_secrets(vault, join_request).await
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
        let joined_device_id = join_request.candidate.device.device_id.clone();
        let local_device_id = self.user_creds.device_id().clone();
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
        let mut ss_log_data = self.get_ss_log_data().await?;
        let mut members = vault.members();
        members.sort_by_key(|member| member.user().device.device_id.to_string());

        for pass_id in vault.secrets.iter() {
            let mut split_claim = ss_log_data
                .claims
                .values()
                .find(|claim| {
                    claim.distribution_type == SecretDistributionType::Split
                        && claim.sender.eq(&local_device_id)
                        && claim.dist_claim_id.pass_id.eq(pass_id)
                })
                .cloned();
            if split_claim.is_none() {
                let local_claim_events = self
                    .p_obj
                    .get_object_events_from_beginning(
                        SsDeviceLogDescriptor::from(local_device_id.clone()),
                    )
                    .await?;
                split_claim = local_claim_events
                    .into_iter()
                    .map(|obj: SsDeviceLogObject| {
                        obj.to_distribution_request()
                    })
                    .find(|claim| {
                        claim.distribution_type == SecretDistributionType::Split
                            && claim.sender.eq(&local_device_id)
                            && claim.dist_claim_id.pass_id.eq(pass_id)
                    });
            }
            let Some(mut split_claim) = split_claim else {
                debug!(
                    "redistribute_existing_secrets: no sender claim found for pass {:?}, skipping",
                    pass_id
                );
                continue;
            };

            let target_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: joined_device_id.clone(),
            });
            let has_target_distribution = self.p_obj.find_tail_event(target_desc).await?.is_some();
            let claim_already_contains_joined =
                split_claim.receivers.iter().any(|d| d.eq(&joined_device_id));
            if has_target_distribution && claim_already_contains_joined {
                continue;
            }

            let source_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass_id.clone(),
                receiver: local_device_id.clone(),
            });
            let Some(source_event) = self.p_obj.find_tail_event(source_desc).await? else {
                debug!(
                    "Skip redistribution for pass {}, source share for local device not found",
                    pass_id.name
                );
                continue;
            };

            let SsWorkflowObject::Distribution(source_dist) = source_event else {
                continue;
            };

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
                debug!(
                    "Skip redistribution for pass {}: not enough shares to recover (need {}, got {})",
                    pass_id.name,
                    shares_needed,
                    shares_for_recovery.len()
                );
                continue;
            }

            let plain_secret = recover_from_shares(shares_for_recovery)?;
            let secure_pass = SecurePassInfo::from(PlainPassInfo {
                pass_id: pass_id.clone(),
                pass: plain_secret.text,
            });
            let re_split = split2(secure_pass, vault.sss_cfg())?;

            if re_split.shares.len() != members.len() {
                bail!("Invalid state: shares count does not match vault members count");
            }

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

            if !split_claim.receivers.iter().any(|d| d.eq(&joined_device_id)) {
                split_claim.receivers.push(joined_device_id.clone());
            }
            split_claim
                .status
                .statuses
                .insert(joined_device_id.clone(), SsDistributionStatus::Pending);
            // Persist the updated claim in the device log too so the next sync
            // sends the new receiver list to the server.
            p_ss.save_claim_in_ss_device_log(split_claim.clone()).await?;
            p_ss.save_ss_log_event(split_claim.clone()).await?;
            ss_log_data.claims.insert(split_claim.id.clone(), split_claim);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo};
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::common::model::secret::SsDistributionId;
    use crate::node::common::model::vault::vault_data::VaultData;
    use crate::node::db::events::shared_secret_event::SsWorkflowObject;
    use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::secret::MetaDistributor;
    use anyhow::Result;

    async fn prepare_single_device_secret() -> Result<(
        FixtureRegistry<EmptyState>,
        MetaOrchestrator<InMemKvLogEventRepo>,
        VaultData,
        MetaPasswordId,
    )> {
        let registry = FixtureRegistry::empty();
        let client_user_creds = registry.state.user_creds.client.clone();
        let client_member = registry.state.vault_data.client_membership.user_data_member();
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
        let seeded_claim = vault_member.create_split_claim(pass_id.clone());
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
}
