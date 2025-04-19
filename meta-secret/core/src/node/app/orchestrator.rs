use crate::node::common::model::secret::{ClaimId, SecretDistributionData, SecretDistributionType, SsClaim, SsLogData};
use crate::node::common::model::user::common::{UserDataMember, UserMembership};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::actions::sign_up::join::AcceptJoinAction;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::SsWorkflowObject;
use crate::node::db::events::vault::vault_log_event::{JoinClusterEvent, VaultActionRequestEvent, VaultLogObject};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::bail;
use anyhow::Result;
use log::{debug, warn};
use std::sync::Arc;

/// Contains business logic of secrets management and login/sign-up actions.
/// Orchestrator is in charge of what is meta secret is made for (the most important part of the app).
/// 1. Passwordless login
/// 2. Decentralized User Management
/// 3. Secret orchestration
pub struct MetaOrchestrator<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user_creds: UserCredentials,
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
                    self.accept_join(join_request).await?;
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
}

impl<Repo: KvLogEventRepo> MetaOrchestrator<Repo> {
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

    pub async fn accept_join(&self, join_request: JoinClusterEvent) -> Result<()> {
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
                        let accept_action = AcceptJoinAction {
                            p_obj: self.p_obj.clone(),
                            member: VaultMember {
                                member: member.clone(),
                                vault: vault.clone(),
                            },
                        };

                        accept_action.accept(db_join_request).await?;
                    }
                }
                VaultActionRequestEvent::AddMetaPass(_) => {
                    //Ignore server side events (no need approval)
                }
            }
        }

        Ok(())
    }

    async fn get_vault(&self, member: UserDataMember) -> Result<VaultData> {
        let p_vault = PersistentVault::from(self.p_obj());
        let vault = p_vault.get_vault(member.user()).await?.to_data();
        Ok(vault)
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
            p_ss
                .get_ss_log_obj(self.user_creds.vault_name.clone())
                .await?
        };
        Ok(ss_log_data)
    }

    /// The device has to send recovery request to the claims' sender
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
