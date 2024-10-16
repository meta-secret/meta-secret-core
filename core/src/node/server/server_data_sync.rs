use std::sync::Arc;

use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserId};
use crate::node::common::model::vault::{VaultData, VaultName, VaultStatus};
use crate::node::db::actions::sign_up::SignUpAction;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::global_index_event::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, Next, ObjectId, UnitId};
use crate::node::db::events::shared_secret_event::{SSDeviceLogObject, SSLedgerObject};
use crate::node::db::events::vault_event::{
    VaultAction, VaultMembershipObject, VaultObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::{SyncRequest, VaultRequest};
use anyhow::{anyhow, bail, Ok};
use async_trait::async_trait;
use tracing::{info, instrument};
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::objects::persistent_vault::PersistentVault;

#[async_trait(? Send)]
pub trait DataSyncApi {
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>>;
    async fn handle(&self, event: GenericKvLogEvent) -> anyhow::Result<()>;
}

pub struct ServerDataSync<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncRequest {
    SyncRequest(SyncRequest),
    ServerTailRequest(UserData),
    Event(GenericKvLogEvent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncResponse {
    Data(DataEventsResponse),
    ServerTailResponse(ServerTailResponse),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEventsResponse(pub Vec<GenericKvLogEvent>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailResponse {
    pub device_log_tail: Option<ObjectId>,
    pub ss_device_log_tail: Option<ObjectId>,
}

impl DataSyncResponse {
    pub fn to_data(&self) -> anyhow::Result<DataEventsResponse> {
        match self {
            DataSyncResponse::Data(data) => Ok(data.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }

    pub fn to_server_tail(&self) -> anyhow::Result<ServerTailResponse> {
        match self {
            DataSyncResponse::ServerTailResponse(server_tail) => Ok(server_tail.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> DataSyncApi for ServerDataSync<Repo> {
    #[instrument(skip(self))]
    async fn replication(&self, request: SyncRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let mut commit_log: Vec<GenericKvLogEvent> = vec![];

        match &request {
            SyncRequest::GlobalIndex(gi_request) => {
                let gi_events = self
                    .global_index_replication(gi_request.global_index.clone())
                    .await?;
                commit_log.extend(gi_events);
            }

            SyncRequest::Vault(vault_request) => {
                let p_vault = PersistentVault {
                    p_obj: self.persistent_obj.clone(),
                };

                let vault_status = p_vault.find(vault_request.sender.clone()).await?;
                match vault_status {
                    VaultStatus::NotExists(_) => {
                        return Ok(commit_log);
                    }
                    VaultStatus::Outsider(_) => {
                        return Ok(commit_log);
                    }
                    VaultStatus::Member { .. } => {
                        let vault_events = self
                            .vault_replication(vault_request.clone())
                            .await?;
                        commit_log.extend(vault_events);
                    }
                }
            }
        }

        Ok(commit_log)
    }

    /// Handle request: all types of requests will be handled and the actions will be executed accordingly
    async fn handle(&self, generic_event: GenericKvLogEvent) -> anyhow::Result<()> {
        self.server_processing(generic_event).await
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    #[instrument(skip(self))]
    async fn server_processing(&self, generic_event: GenericKvLogEvent) -> anyhow::Result<()> {
        match &generic_event {
            GenericKvLogEvent::DeviceLog(device_log_obj) => {
                self.handle_device_log_request(device_log_obj).await?;
            }
            GenericKvLogEvent::SSDeviceLog(ss_device_log_obj) => {
                info!("Shared Secret Device Log message processing: {:?}", &ss_device_log_obj);

                self.persistent_obj
                    .repo
                    .save(ss_device_log_obj.clone().to_generic())
                    .await?;

                if let SSDeviceLogObject::SSDeviceLog(event) = ss_device_log_obj {
                    let ss_claim = event.value.clone();

                    let ss_ledger_desc = SharedSecretDescriptor::SSLedger(ss_claim.vault_name.clone())
                        .to_obj_desc();

                    let maybe_generic_ss_ledger = self.persistent_obj
                        .find_tail_event(ss_ledger_desc.clone())
                        .await?;

                    match maybe_generic_ss_ledger {
                        Some(generic_ss_ledger) => {
                            let ss_ledger_obj = SSLedgerObject::try_from(generic_ss_ledger)?;

                            let mut ss_ledger = ss_ledger_obj.to_ledger_data()?;
                            if ss_ledger.claims.contains_key(&ss_claim.id) {
                                //the claim is already in the ledger, no action needed
                                return Ok(());
                            } else {
                                //add the claim to the ledger
                                ss_ledger.claims.insert(ss_claim.id.clone(), ss_claim.clone());

                                //update ss_ledger
                                let updated_ss_ledger = SSLedgerObject::Ledger(KvLogEvent {
                                    key: KvKey {
                                        obj_id: ss_ledger_obj.get_ledger_id()?.next(),
                                        obj_desc: ss_ledger_desc,
                                    },
                                    value: ss_ledger,
                                });

                                self.persistent_obj.repo.save(updated_ss_ledger.to_generic()).await?;
                            }
                        }
                        None => {
                            unimplemented!("Not implemented yet")
                        }
                    }
                }
            }
            _ => {
                bail!("Invalid event type: {:?}", generic_event);
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn handle_device_log_request(&self, device_log_obj: &DeviceLogObject) -> anyhow::Result<()> {
        self.persistent_obj
            .repo
            .save(device_log_obj.clone().to_generic())
            .await?;

        let vault_action_event = match device_log_obj {
            DeviceLogObject::Unit { .. } => {
                return Ok(());
            }
            DeviceLogObject::Genesis(_) => {
                return Ok(());
            }
            DeviceLogObject::Action(event) => event,
        };

        let vault_action = vault_action_event.value.clone();

        match &vault_action {
            VaultAction::CreateVault(user) => {
                // create vault if not exists
                let p_vault = PersistentVault {
                    p_obj: self.persistent_obj.clone(),
                };

                let vault_status = p_vault.find(user.clone()).await?;
                match vault_status {
                    VaultStatus::NotExists(_) => {
                        //create vault_log, vault and vault status
                        self.create_vault(user.clone()).await
                    }
                    VaultStatus::Outsider(_) => {
                        // vault already exists, and the event have been saved into vault_log already,
                        // no action needed
                        Ok(())
                    }
                    VaultStatus::Member { .. } => {
                        // vault already exists, and the event have been saved into vault_log already,
                        // no action needed
                        Ok(())
                    }
                }
            }

            VaultAction::JoinClusterRequest { candidate } => {
                let vault_log_desc = VaultDescriptor::VaultLog(candidate.vault_name.clone())
                    .to_obj_desc();

                let vault_log_free_id = self
                    .persistent_obj
                    .find_free_id_by_obj_desc(vault_log_desc.clone())
                    .await?;

                if let ObjectId::Artifact(vault_log_artifact_id) = vault_log_free_id {
                    let action = VaultLogObject::Action(KvLogEvent {
                        key: KvKey::artifact(vault_log_desc, vault_log_artifact_id),
                        value: vault_action.clone(),
                    });
                    let vault_log_action_event = GenericKvLogEvent::VaultLog(action);

                    self.persistent_obj.repo.save(vault_log_action_event).await?;
                    Ok(())
                } else {
                    bail!(
                        "JoinClusterRequest: Invalid vault log id, must be ArtifactId, but it's: {:?}",
                        vault_log_free_id
                    );
                }
            }

            VaultAction::UpdateMembership {
                sender: UserDataMember(sender_user),
                update,
            } => {
                let vault_name = vault_action.vault_name();
                //check if a sender is a member of the vault and update the vault then
                let vault_log_desc = VaultDescriptor::VaultLog(vault_name).to_obj_desc();

                let vault_log_free_id = self
                    .persistent_obj
                    .find_free_id_by_obj_desc(vault_log_desc.clone())
                    .await?;

                let ObjectId::Artifact(_) = vault_log_free_id else {
                    bail!(
                        "UpdateMembership: Invalid vault log id, must be ArtifactId, but it's: {:?}",
                        vault_log_free_id
                    );
                };

                let vault_name = sender_user.vault_name.clone();
                let (vault_artifact_id, vault) = self.get_vault(&sender_user).await?;

                let vault_event = {
                    let mut new_vault = vault.clone();
                    new_vault.update_membership(update.clone());

                    let key = KvKey {
                        obj_id: vault_artifact_id,
                        obj_desc: VaultDescriptor::vault(vault_name.clone()),
                    };
                    VaultObject::Vault(KvLogEvent { key, value: new_vault }).to_generic()
                };

                self.persistent_obj.repo.save(vault_event).await?;

                // Don't forget to update the vault status

                let vault_status_desc = {
                    let user_id = UserId {
                        device_id: update.device_id(),
                        vault_name,
                    };
                    VaultDescriptor::VaultMembership(user_id).to_obj_desc()
                };

                let vault_status_free_id = self
                    .persistent_obj
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

                self.persistent_obj.repo.save(vault_status_event).await?;
                Ok(())
            }
            VaultAction::AddMetaPassword { sender, meta_pass_id } => {
                let user = sender.user();
                let (vault_artifact_id, vault) = self.get_vault(&user).await?;

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

                self.persistent_obj.repo.save(vault_event).await?;

                Ok(())
            }
        }
    }

    async fn get_vault(&self, user_data: &UserData) -> anyhow::Result<(ArtifactId, VaultData)> {
        let p_vault = PersistentVault {
            p_obj: self.persistent_obj.clone(),
        };

        let vault_status = p_vault.find(user_data.clone()).await?;
        match vault_status {
            VaultStatus::NotExists(_) => {
                Err(anyhow!("Vault not found"))
            }
            VaultStatus::Outsider(_) => {
                Err(anyhow!("Sender is not a member of the vault"))
            }
            VaultStatus::Member { vault, .. } => {
                //save new vault state
                let vault_desc = VaultDescriptor::vault(vault.vault_name.clone());

                let vault_free_id = self.persistent_obj
                    .find_free_id_by_obj_desc(vault_desc.clone())
                    .await?;

                let ObjectId::Artifact(vault_artifact_id) = vault_free_id else {
                    return Err(anyhow!("Invalid vault id, must be ArtifactId, but it's: {:?}",vault_free_id));
                };

                Ok((vault_artifact_id, vault))
            }
        }
    }

    #[instrument(skip_all)]
    async fn global_index_replication(&self, gi_id: ObjectId) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        let events = self.persistent_obj.find_object_events(gi_id).await?;
        Ok(events)
    }

    pub async fn vault_replication(&self, request: VaultRequest) -> anyhow::Result<Vec<GenericKvLogEvent>> {
        info!("Vault replication, by: {:?}", request.sender);
        let mut commit_log = vec![];

        //sync VaultLog
        {
            let vault_log_events = self
                .persistent_obj
                .find_object_events(request.vault_log.clone())
                .await?;
            info!("Server. Sync vault_log. Request: {:?}, events: {:?}", &request.vault_log, &vault_log_events);
            commit_log.extend(vault_log_events);
        }

        //sync Vault
        {
            let vault_events = self.persistent_obj.find_object_events(request.vault.clone()).await?;
            commit_log.extend(vault_events);
        }

        //sync vault status
        {
            let vault_status_events = self
                .persistent_obj
                .find_object_events(request.vault_status.clone())
                .await?;

            commit_log.extend(vault_status_events);
        }

        Ok(commit_log)
    }
}

impl<Repo: KvLogEventRepo> ServerDataSync<Repo> {
    async fn create_vault(&self, candidate: UserData) -> anyhow::Result<()> {
        //vault not found, we can create our new vault
        info!("Accept SignUp request, for the vault: {:?}", candidate.vault_name());

        let creds_repo = PersistentCredentials { p_obj: self.persistent_obj.clone() };
        let device_creds = creds_repo.get_or_generate_device_creds(DeviceName::server()).await?;

        let server = device_creds.device.clone();

        let sign_up_action = SignUpAction {};
        let sign_up_events = sign_up_action.accept(candidate.clone(), server.clone());

        for sign_up_event in sign_up_events {
            self.persistent_obj.repo.save(sign_up_event).await?;
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
                    obj_desc: GlobalIndexDescriptor::Index.to_obj_desc(),
                },
                value: vault_id.clone(),
            })
                .to_generic()
        };

        let gi_events = vec![gi_update_event];

        for gi_event in gi_events {
            self.persistent_obj.repo.save(gi_event).await?;
        }

        let vault_idx_evt = GlobalIndexObject::index_from_vault_id(vault_id).to_generic();

        self.persistent_obj.repo.save(vault_idx_evt).await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::setup_tracing;

    #[tokio::test]
    async fn test() -> anyhow::Result<()> {
        setup_tracing()?;

        let registry = FixtureRegistry::extended().await?;


        Ok(())
    }
}