use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::actions::sign_up::action::SignUpAction;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::{VaultDescriptor, VaultMembershipDescriptor};
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::{VaultActionEvent, VaultActionUpdateEvent};
use crate::node::db::events::vault::vault_membership::VaultMembershipObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

pub struct ServerVaultAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> ServerVaultAction<Repo> {
    pub async fn do_processing(&self, action_event: VaultActionEvent) -> Result<()> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        match &action_event {
            VaultActionEvent::Request(_) => {
                //saving messages from device_log to vault_log guarantees ordering between events
                //sent from different devices simultaneously
                p_vault.save_vault_log_event(action_event).await?;
                anyhow::Ok(())
            }
            VaultActionEvent::Update(action_update) => {
                match action_update {
                    VaultActionUpdateEvent::CreateVault { owner } => {
                        let action = CreateVaultAction {
                            p_obj: self.p_obj.clone(),
                            server_device: self.server_device.clone(),
                        };
                        action.create(&owner).await
                    }
                    VaultActionUpdateEvent::UpdateMembership { sender, update } => {
                        let vault_name = action_event.vault_name();
                        //check if a sender is a member of the vault and update the vault then
                        let (vault_artifact_id, vault) = p_vault.get_vault(sender.user()).await?;

                        let vault_event = {
                            let new_vault = vault.update_membership(update.clone());

                            let key = KvKey {
                                obj_id: vault_artifact_id,
                                obj_desc: VaultDescriptor::from(vault_name).to_obj_desc(),
                            };
                            VaultObject::Vault(KvLogEvent {
                                key,
                                value: new_vault,
                            })
                        };

                        self.p_obj.repo.save(vault_event).await?;

                        //update vault status accordingly
                        let vault_status_free_id = {
                            let vault_membership_desc =
                                { VaultMembershipDescriptor::from(update.user_data().user_id()) };

                            self.p_obj
                                .find_free_id_by_obj_desc(vault_membership_desc.clone())
                                .await?
                        };

                        let vault_status_events = match vault_status_free_id {
                            ObjectId::Unit(_) => VaultMembershipObject::init(update.user_data()),
                            ObjectId::Genesis(artifact_id) => {
                                let genesis = VaultMembershipObject::genesis(update.user_data());
                                let member = VaultMembershipObject::member(
                                    update.user_data(),
                                    artifact_id.next(),
                                );
                                vec![genesis.to_generic(), member.to_generic()]
                            }
                            ObjectId::Artifact(artifact_id) => {
                                let event =
                                    VaultMembershipObject::membership(update.clone(), artifact_id)
                                        .to_generic();
                                vec![event]
                            }
                        };

                        for vault_status_event in vault_status_events {
                            self.p_obj.repo.save(vault_status_event).await?;
                        }
                        Ok(())
                    }
                    VaultActionUpdateEvent::AddMetaPass {
                        sender,
                        meta_pass_id,
                    } => {
                        let user = sender.user();
                        let (vault_artifact_id, vault) = p_vault.get_vault(user).await?;

                        let vault_event = {
                            let new_vault = vault.add_secret(meta_pass_id.clone());

                            let obj_desc =
                                VaultDescriptor::from(user.vault_name.clone()).to_obj_desc();

                            let event = KvLogEvent {
                                key: KvKey {
                                    obj_id: vault_artifact_id,
                                    obj_desc,
                                },
                                value: new_vault,
                            };

                            VaultObject::Vault(event)
                        };

                        self.p_obj.repo.save(vault_event).await?;

                        Ok(())
                    }
                }
            }
        }
    }
}

pub struct CreateVaultAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> CreateVaultAction<Repo> {
    pub async fn create(&self, owner: &UserData) -> Result<()> {
        // create vault if not exists
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(owner.clone()).await?;
        if let VaultStatus::NotExists(_) = vault_status {
            //create vault_log, vault and vault status
            self.create_vault(owner.clone()).await
        } else {
            // vault already exists, and the event have been saved into vault_log already,
            // no action needed
            anyhow::Ok(())
        }
    }

    async fn create_vault(&self, candidate: UserData) -> Result<()> {
        //vault not found, we can create our new vault
        info!(
            "Accept SignUp request, for the vault: {:?}",
            candidate.vault_name()
        );

        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(candidate.clone(), self.server_device.clone());

        for sign_up_event in sign_up_events {
            self.p_obj.repo.save(sign_up_event).await?;
        }
        anyhow::Ok(())
    }
}
