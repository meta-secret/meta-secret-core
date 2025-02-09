use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::common::model::vault::vault_data::VaultAggregate;
use crate::node::db::actions::sign_up::action::SignUpAction;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::{VaultDescriptor, VaultMembershipDescriptor};
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, VaultActionEvent, VaultActionInitEvent, VaultActionUpdateEvent,
};
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

        //saving messages from device_log to vault_log guarantees ordering between events
        //sent from different devices simultaneously
        match &action_event {
            VaultActionEvent::Init(VaultActionInitEvent::CreateVault(create_vault_event)) => {
                let action = CreateVaultAction {
                    p_obj: self.p_obj.clone(),
                    server_device: self.server_device.clone(),
                };
                let _ = action.create(&create_vault_event.owner).await;
            }

            VaultActionEvent::Request(action_request) => {
                p_vault
                    .save_vault_log_request_event(action_request.clone())
                    .await?;
            }
            VaultActionEvent::Update(action_update) => {
                let vault_name = action_update.vault_name();
                //check if a sender is a member of the vault and update the vault then
                let (vault_artifact_id, vault) =
                    p_vault.get_vault(action_update.sender().user()).await?;

                let vault_action_events = p_vault
                    .get_vault_log_artifact(action_event.vault_name())
                    .await?
                    .0
                    .value
                    .apply(action_update.clone());

                let agg = VaultAggregate::build_from(vault_action_events, vault);

                let vault_event = {
                    let key = KvKey {
                        obj_id: vault_artifact_id,
                        obj_desc: VaultDescriptor::from(vault_name.clone()).to_obj_desc(),
                    };
                    VaultObject(KvLogEvent {
                        key,
                        value: agg.vault,
                    })
                };

                self.p_obj.repo.save(vault_event).await?;

                p_vault
                    .save_vault_log_events(agg.events, vault_name)
                    .await?;

                match action_update {
                    VaultActionUpdateEvent::UpdateMembership { update, .. } => {
                        //update vault status accordingly
                        let free_id = {
                            let vault_membership_desc =
                                VaultMembershipDescriptor::from(update.user_data().user_id());

                            self.p_obj
                                .find_free_id_by_obj_desc(vault_membership_desc.clone())
                                .await?
                        };

                        let event = {
                            let membership = VaultMembershipObject::new(update.clone(), free_id);
                            membership.to_generic()
                        };

                        self.p_obj.repo.save(event).await?;
                    }
                    VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent { .. }) => {
                        // no extra steps required
                    }
                }
            }
        }

        Ok(())
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
        let sign_up_events = sign_up_action.accept(candidate.clone());

        for sign_up_event in sign_up_events {
            self.p_obj.repo.save(sign_up_event).await?;
        }
        anyhow::Ok(())
    }
}
