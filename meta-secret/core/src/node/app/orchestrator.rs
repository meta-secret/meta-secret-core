use crate::node::common::model::secret::{SecretDistributionData, SecretDistributionType};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::vault::{VaultMember, VaultStatus};
use crate::node::db::actions::sign_up::join::AcceptJoinAction;
use crate::node::db::descriptors::shared_secret_descriptor::SsDescriptor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::SsDistributionObject;
use crate::node::db::events::vault::vault_log_event::{VaultActionRequestEvent, VaultLogObject};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::bail;
use log::warn;
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
    pub async fn orchestrate(&self) -> anyhow::Result<()> {
        let p_vault = PersistentVault::from(self.p_obj());
        let vault_status = p_vault.find(self.user_creds.user()).await?;

        let VaultStatus::Member(member) = vault_status else {
            warn!("Not a vault member");
            return Ok(());
        };

        let maybe_vault_log_event = {
            let vault_name = member.user_data.vault_name();
            p_vault.vault_log(vault_name).await?
        };

        if let Some(VaultLogObject(action_event)) = maybe_vault_log_event {
            let vault_actions = action_event.value;

            let vault = p_vault.get_vault(member.user()).await?.to_data();

            for request in vault_actions.requests {
                match request {
                    VaultActionRequestEvent::JoinCluster(join_request) => {
                        let accept_action = AcceptJoinAction {
                            p_obj: self.p_obj.clone(),
                            member: VaultMember {
                                member: member.clone(),
                                vault: vault.clone(),
                            },
                        };

                        accept_action.accept(join_request).await?;
                    }
                    VaultActionRequestEvent::AddMetaPass(_) => {
                        //Ignore server side events (no need approval)
                    }
                }
            }
        }

        // shared secret actions
        let p_ss = PersistentSharedSecret::from(self.p_obj.clone());

        let ss_log_data = p_ss
            .get_ss_log_obj(self.user_creds.vault_name.clone())
            .await?;

        for (_, claim) in ss_log_data.claims {
            if claim.distribution_type != SecretDistributionType::Recover {
                continue;
            }

            //get distributions
            for claim_db_id in claim.claim_db_ids() {
                //get distribution id
                let local_device = &self.user_creds.device_creds.device.device_id;
                if claim_db_id.distribution_id.receiver.eq(local_device) {
                    let ss_obj = p_ss
                        .get_ss_distribution_event_by_id(claim_db_id.distribution_id.clone())
                        .await?;

                    let SsDistributionObject::Distribution(dist_event) = ss_obj else {
                        bail!("Ss distribution object not found");
                    };

                    let KvLogEvent { value: share, .. } = dist_event;

                    // re encrypt message?
                    let msg_receiver = share.secret_message.cipher_text().channel.receiver();
                    let msg_receiver_device = msg_receiver.to_device_id();

                    let msg = if msg_receiver_device.eq(&claim.sender) {
                        //just send already encrypted message back
                        share.secret_message
                    } else {
                        // re-encrypt!
                        self.user_creds
                            .device_creds
                            .secret_box
                            .re_encrypt(share.secret_message.clone(), msg_receiver)?
                    };

                    //compare with claim dist id, if match then create a claim
                    let key = KvKey::from(SsDescriptor::Claim(claim_db_id.clone()));

                    let new_claim = SsDistributionObject::Claim(KvLogEvent {
                        key,
                        value: SecretDistributionData {
                            vault_name: self.user_creds.vault_name.clone(),
                            claim_id: claim_db_id.claim_id,
                            secret_message: msg,
                        },
                    });

                    p_ss.p_obj.repo.save(new_claim).await?;
                }
            }
        }

        Ok(())
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.p_obj.clone()
    }
}
