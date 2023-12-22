use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tracing::{debug, info, instrument};

use crate::node::common::model::device::{DeviceCredentials, DeviceData};
use crate::node::common::model::user::{UserData, UserDataMember, UserId};
use crate::node::common::model::vault::{VaultData, VaultName};
use crate::node::db::actions::sign_up::SignUpAction;
use crate::node::db::actions::ss_replication::SSReplicationAction;
use crate::node::db::descriptors::global_index::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::global_index::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId, UnitId};
use crate::node::db::events::vault_event::{DeviceLogObject, VaultAction, VaultLogObject, VaultMembershipObject, VaultObject};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::{SyncRequest, VaultRequest};

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>>;
    async fn send(&self, event: GenericKvLogEvent) -> anyhow::Result<()>;
}

pub struct ServerDataSync<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    pub device_creds: DeviceCredentials,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncRequest {
    SyncRequest(SyncRequest),
    Event(GenericKvLogEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSyncResponse {
    pub events: Vec<GenericKvLogEvent>,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> DataSyncApi for ServerDataSync<Repo> {
    #[instrument(skip(self))]
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        match request {
            SyncRequest::GlobalIndex(gi_request) => {
                let gi_events = self
                    .global_index_replication(gi_request.global_index.clone())
                    .await?;
                commit_log.extend(gi_events);
            }

            SyncRequest::Vault(vault_request) => {
                let maybe_vault = self.find_vault(vault_request.vault.clone()).await?;
                let Some(vault) = maybe_vault else {
                    return Ok(commit_log);
                };

                if !vault.is_member(&vault_request.sender.device.id) {
                    return Ok(commit_log);
                }

                let vault_events = self
                    .vault_replication(vault_request)
                    .await?;

                commit_log.extend(vault_events);
            }

            SyncRequest::SharedSecret(ss_request) => {
                let maybe_vault = self.find_vault(ss_request.ss_log.clone()).await?;
                let Some(vault) = maybe_vault else {
                    return Ok(commit_log);
                };

                if !vault.is_member(&ss_request.sender.device.id) {
                    return Ok(commit_log);
                }

                let s_s_replication_action = SSReplicationAction {
                    persistent_obj: self.persistent_obj.clone(),
                };
                let s_s_replication_events = s_s_replication_action
                    .replicate(ss_request, &vault)
                    .await?;

                commit_log.extend(s_s_replication_events);
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn send(&self, generic_event: GenericKvLogEvent) -> anyhow::Result<()> {
        self.server_processing(generic_event).await?;
        Ok(())
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    #[instrument(skip_all)]
    async fn server_processing(&self, generic_event: GenericKvLogEvent) -> anyhow::Result<()> {
        debug!("DataSync::event_processing: {:?}", &generic_event);

        match &generic_event {
            GenericKvLogEvent::GlobalIndex(_) => {
                info!("Global index not allowed to be sent");
            }
            GenericKvLogEvent::DbTail(_) => {
                info!("DbTail not allowed to be sent");
            }
            GenericKvLogEvent::Credentials(_) => {
                info!("Credentials not allowed to be sent");
            }
            GenericKvLogEvent::DeviceLog(device_log_obj) => {
                self.persistent_obj.repo
                    .save(generic_event.clone())
                    .await?;

                let vault_action_event = match device_log_obj {
                    DeviceLogObject::Unit { .. } => {
                        return Ok(());
                    }
                    DeviceLogObject::Genesis(_) => {
                        return Ok(());
                    }
                    DeviceLogObject::Action(event) => {
                        event
                    }
                };

                let vault_action = vault_action_event.value.clone();

                let vault_log_desc = VaultDescriptor::VaultLog(vault_action.vault_name())
                    .to_obj_desc();

                let vault_log_free_id = self.persistent_obj
                    .find_free_id_by_obj_desc(vault_log_desc.clone())
                    .await?;

                let ObjectId::Artifact(vault_log_artifact_id) = vault_log_free_id else {
                    return Err(anyhow!("Vault log invalid state: {:?}", vault_log_free_id));
                };

                let vault_log_action_event = GenericKvLogEvent::VaultLog(VaultLogObject::Action(KvLogEvent {
                    key: KvKey::artifact(vault_log_desc, vault_log_artifact_id),
                    value: vault_action.clone(),
                }));

                self.persistent_obj
                    .repo
                    .save(vault_log_action_event)
                    .await?;

                match vault_action {
                    VaultAction::JoinRequest { candidate } => {
                        // create vault if not exists
                        let vault_name = candidate.vault_name.clone();
                        let vault_desc = VaultDescriptor::vault(vault_name.clone());
                        let maybe_vault = self
                            .find_vault(ObjectId::unit(vault_desc.clone()))
                            .await?;

                        if let Some(_vault) = maybe_vault {
                            return Ok(());
                        };

                        //create vault_log, vault and vault status
                        self.accept_sign_up_request(candidate).await?
                    }

                    VaultAction::UpdateMembership {
                        sender: UserDataMember(sender_user),
                        update
                    } => {
                        //check if a sender is a member of the vault and update the vault then

                        let vault_name = sender_user.vault_name.clone();
                        let (vault_artifact_id, vault) = self
                            .get_vault(vault_name.clone(), &sender_user.device)
                            .await?;

                        let vault_event = {
                            let mut new_vault = vault.clone();
                            new_vault.update_membership(update.clone());

                            VaultObject::Vault(KvLogEvent {
                                key: KvKey {
                                    obj_id: vault_artifact_id,
                                    obj_desc: VaultDescriptor::vault(vault_name.clone()),
                                },
                                value: new_vault,
                            }).to_generic()
                        };

                        self.persistent_obj
                            .repo
                            .save(vault_event)
                            .await?;

                        // Don't forget to update the vault status

                        let vault_status_desc = {
                            let user_id = UserId {
                                device_id: update.device_id(),
                                vault_name,
                            };
                            VaultDescriptor::VaultStatus(user_id).to_obj_desc()
                        };

                        let vault_status_free_id = self.persistent_obj
                            .find_free_id_by_obj_desc(vault_status_desc.clone())
                            .await?;

                        let ObjectId::Artifact(vault_status_artifact_id) = vault_status_free_id else {
                            return Ok(());
                        };

                        let vault_status_event = {
                            let event = KvLogEvent {
                                key: KvKey::artifact(vault_status_desc, vault_status_artifact_id),
                                value: update.clone(),
                            };
                            VaultMembershipObject::Membership(event).to_generic()
                        };

                        self.persistent_obj
                            .repo
                            .save(vault_status_event)
                            .await?;
                    }
                    VaultAction::AddMetaPassword { sender, meta_pass_id } => {
                        let user = sender.user();
                        let vault_name = user.vault_name.clone();
                        let (vault_artifact_id, vault) = self
                            .get_vault(vault_name.clone(), &user.device)
                            .await?;

                        let vault_event = {
                            let mut new_vault = vault.clone();
                            new_vault.add_secret(meta_pass_id.clone());

                            let event = KvLogEvent {
                                key: KvKey {
                                    obj_id: vault_artifact_id,
                                    obj_desc: VaultDescriptor::vault(vault_name.clone()),
                                },
                                value: new_vault,
                            };

                            VaultObject::Vault(event).to_generic()
                        };

                        self.persistent_obj
                            .repo
                            .save(vault_event)
                            .await?;
                    }
                }
            }
            GenericKvLogEvent::SharedSecret(_) => {
                todo!("Implement shared secret distribution");
            }
            GenericKvLogEvent::VaultLog(_) => {
                info!("VaultLog can be updated only by the server");
            }
            GenericKvLogEvent::Vault(_) => {
                info!("Vault can be updated only by the server");
            }
            GenericKvLogEvent::VaultMembership(_) => {
                info!("VaultStatus can be updated only by the server");
            }
            GenericKvLogEvent::Error { .. } => {
                info!("Errors not yet implemented");
            }
            GenericKvLogEvent::SSDeviceLog(_) => {
                info!("SSLog can be updated only by the server itself");
            }
        }

        Ok(())
    }

    async fn get_vault(&self, vault_name: VaultName, sender_device: &DeviceData) -> anyhow::Result<(ArtifactId, VaultData)> {
        let vault_desc = VaultDescriptor::vault(vault_name.clone());
        let maybe_vault = self.find_vault(ObjectId::unit(vault_desc.clone())).await?;
        let Some(vault) = maybe_vault else {
            return Err(anyhow!("Vault not found"));
        };

        if !vault.is_member(&sender_device.id) {
            return Err(anyhow!("Sender is not a member of the vault"));
        }

        //save new vault state
        let vault_free_id = self.persistent_obj
            .find_free_id_by_obj_desc(vault_desc.clone())
            .await?;

        let ObjectId::Artifact(vault_artifact_id) = vault_free_id else {
            return Err(anyhow!("Invalid vault id, must be ArtifactId, but it's: {:?}", vault_free_id));
        };

        Ok((vault_artifact_id, vault))
    }

    #[instrument(skip_all)]
    async fn global_index_replication(&self, gi_id: ObjectId) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let events = self.persistent_obj
            .find_object_events(gi_id)
            .await?;
        Ok(events)
    }

    pub async fn vault_replication(&self, request: VaultRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log = vec![];

        //sync VaultLog
        {
            let vault_log_events = self.persistent_obj
                .find_object_events(request.vault_log.clone())
                .await?;
            commit_log.extend(vault_log_events);
        }

        //sync Vault
        {
            let vault_events = self.persistent_obj
                .find_object_events(request.vault.clone())
                .await?;

            commit_log.extend(vault_events);
        }

        //sync vault status
        {
            let vault_status_events = self.persistent_obj
                .find_object_events(request.vault_status.clone())
                .await?;

            commit_log.extend(vault_status_events);
        }

        Ok(commit_log)
    }

    async fn find_vault(&self, vault_id: ObjectId) -> anyhow::Result<Option<VaultData>> {
        let maybe_vault_event = self.persistent_obj
            .find_tail_event_by_obj_id(vault_id)
            .await?;

        let Some(vault_event) = maybe_vault_event else {
            return Ok(None);
        };

        let GenericKvLogEvent::Vault(VaultObject::Vault(event)) = vault_event else {
            return Ok(None);
        };

        let vault = event.value;
        Ok(Some(vault))
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    async fn accept_sign_up_request(&self, candidate: UserData) -> anyhow::Result<()> {
        //vault not found, we can create our new vault
        info!("Accept SignUp request, for the vault: {:?}", candidate.vault_name());

        let server = self.device_creds.device.clone();

        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(candidate.clone(), server.clone());

        for sign_up_event in sign_up_events {
            self.persistent_obj
                .repo
                .save(sign_up_event)
                .await?;
        }

        self.update_global_index(candidate.vault_name()).await?;

        Ok(())
    }

    async fn update_global_index(&self, vault_name: VaultName) -> anyhow::Result<()> {
        //find the latest global_index_id???
        let gi_free_id = self
            .persistent_obj
            .find_free_id_by_obj_desc(ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index))
            .await?;

        let ObjectId::Artifact(gi_artifact_id) = gi_free_id else {
            return Err(anyhow!("Invalid global index state"));
        };

        let vault_id = UnitId::vault_unit(vault_name.clone());

        let gi_update_event = {
            GlobalIndexObject::Update(KvLogEvent {
                key: KvKey {
                    obj_id: gi_artifact_id,
                    obj_desc: GlobalIndexDescriptor::Index.to_obj_desc()
                },
                value: vault_id.clone(),
            }).to_generic()
        };

        let gi_events = vec![gi_update_event];

        for gi_event in gi_events {
            self.persistent_obj
                .repo
                .save(gi_event)
                .await?;
        }

        let vault_idx_evt = GlobalIndexObject::index_from_vault_id(vault_id).to_generic();

        self.persistent_obj.repo.save(vault_idx_evt).await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {

}