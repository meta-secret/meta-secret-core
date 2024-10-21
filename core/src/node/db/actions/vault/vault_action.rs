use std::sync::Arc;
use anyhow::bail;
use tracing::info;
use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserId};
use crate::node::common::model::vault::{VaultName, VaultStatus};
use crate::node::db::actions::sign_up::action::SignUpAction;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{Next, ObjectId};
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault_event::{VaultActionEvent, VaultMembershipObject, VaultObject};
use crate::node::db::objects::global_index::ServerPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct VaultAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData
}

impl<Repo: KvLogEventRepo> VaultAction<Repo> {
    
    pub async fn do_processing(&self, action_event: VaultActionEvent) -> anyhow::Result<()> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };
        
        match &action_event {
            VaultActionEvent::CreateVault(user) => {
                let action = CreateVaultAction {
                    p_obj: self.p_obj.clone(),
                    server_device: self.server_device.clone(),
                };
                action.create(user).await
            }

            VaultActionEvent::JoinClusterRequest { candidate } => {
                let vault_log_desc = VaultDescriptor::vault_log(candidate.vault_name.clone());

                let vault_log_free_id = self
                    .p_obj
                    .find_free_id_by_obj_desc(vault_log_desc.clone())
                    .await?;

                if let ObjectId::Artifact(vault_log_artifact_id) = vault_log_free_id {
                    let action = VaultLogObject::Action(KvLogEvent {
                        key: KvKey::artifact(vault_log_desc, vault_log_artifact_id),
                        value: action_event.clone(),
                    });
                    let vault_log_event = GenericKvLogEvent::VaultLog(action);

                    self.p_obj.repo.save(vault_log_event).await?;
                    anyhow::Ok(())
                } else {
                    bail!(
                        "JoinClusterRequest: Invalid vault log id, must be ArtifactId, but it's: {:?}",
                        vault_log_free_id
                    );
                }
            }

            VaultActionEvent::UpdateMembership {
                sender: UserDataMember(sender_user),
                update,
            } => {
                let vault_name = action_event.vault_name();
                //check if a sender is a member of the vault and update the vault then
                let vault_log_desc = VaultDescriptor::VaultLog(vault_name.clone()).to_obj_desc();

                let vault_log_free_id = self
                    .p_obj
                    .find_free_id_by_obj_desc(vault_log_desc.clone())
                    .await?;

                let ObjectId::Artifact(_) = vault_log_free_id else {
                    bail!(
                        "UpdateMembership: Invalid vault log id, must be ArtifactId, but it's: {:?}",
                        vault_log_free_id
                    );
                };
                
                let (vault_artifact_id, vault) = p_vault.get_vault(&sender_user).await?;

                let vault_event = {
                    let mut new_vault = vault.clone();
                    new_vault.update_membership(update.clone());

                    let key = KvKey {
                        obj_id: vault_artifact_id,
                        obj_desc: VaultDescriptor::vault(vault_name.clone()),
                    };
                    VaultObject::Vault(KvLogEvent { key, value: new_vault }).to_generic()
                };

                self.p_obj.repo.save(vault_event).await?;

                let vault_status_free_id = {
                    let vault_membership_desc = {
                        VaultDescriptor::VaultMembership(update.user_data().user_id()).to_obj_desc()
                    };
                    
                    self
                        .p_obj
                        .find_free_id_by_obj_desc(vault_membership_desc.clone())
                        .await?
                };

                let vault_status_events = match vault_status_free_id {
                    ObjectId::Unit(_) => {
                        VaultMembershipObject::init(update.user_data())
                    }
                    ObjectId::Genesis(artifact_id) => {
                        let genesis = VaultMembershipObject::genesis(update.user_data()).to_generic();
                        let member = VaultMembershipObject::member(update.user_data(), artifact_id.next())
                            .to_generic();
                        vec![genesis, member]
                    }
                    ObjectId::Artifact(artifact_id) => {
                        let event = VaultMembershipObject::membership(update.clone(), artifact_id)
                            .to_generic();
                        vec![event]
                    }
                };

                for vault_status_event in vault_status_events {
                    self.p_obj.repo.save(vault_status_event).await?;
                }
                anyhow::Ok(())
            }
            
            VaultActionEvent::AddMetaPassword { sender, meta_pass_id } => {
                
                let user = sender.user();
                let (vault_artifact_id, vault) = p_vault.get_vault(&user).await?;

                let vault_event = {
                    let mut new_vault = vault.clone();
                    new_vault.add_secret(meta_pass_id.clone());

                    let event = KvLogEvent {
                        key: KvKey {
                            obj_id: vault_artifact_id,
                            obj_desc: VaultDescriptor::vault(user.vault_name.clone()),
                        },
                        value: new_vault,
                    };

                    VaultObject::Vault(event).to_generic()
                };

                self.p_obj.repo.save(vault_event).await?;

                anyhow::Ok(())
            }
        }
    }
}


pub struct CreateVaultAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData
}

impl<Repo: KvLogEventRepo> CreateVaultAction<Repo> {
    pub async fn create(&self, user: &UserData) -> anyhow::Result<()> {
        // create vault if not exists
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(user.clone()).await?;
        if let VaultStatus::NotExists(_) = vault_status {
            //create vault_log, vault and vault status
            self.create_vault(user.clone()).await
        } else {
            // vault already exists, and the event have been saved into vault_log already,
            // no action needed
            anyhow::Ok(())
        }
    }

    async fn create_vault(&self, candidate: UserData) -> anyhow::Result<()> {
        //vault not found, we can create our new vault
        info!("Accept SignUp request, for the vault: {:?}", candidate.vault_name());

        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(candidate.clone(), self.server_device.clone());

        for sign_up_event in sign_up_events {
            self.p_obj.repo.save(sign_up_event).await?;
        }

        let p_gi = ServerPersistentGlobalIndex {
            p_obj: self.p_obj.clone(),
            server_device: self.server_device.clone() ,
        };
        p_gi.update(candidate.vault_name()).await?;

        anyhow::Ok(())
    }
}
