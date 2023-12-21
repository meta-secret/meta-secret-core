use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::node::app::meta_app::meta_client_service::MetaClientAccessProxy;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::vault_event::{VaultAction, VaultLogObject};
use crate::node::db::objects::device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::shared_secret::PersistentSharedSecret;
use crate::node::db::objects::vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::server_app::ServerDataTransfer;

pub struct VirtualDevice<Repo: KvLogEventRepo> {
    persistent_object: Arc<PersistentObject<Repo>>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub server_dt: Arc<ServerDataTransfer>,
    gateway: Arc<SyncGateway<Repo>>
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp,
}

impl<Repo: KvLogEventRepo> VirtualDevice<Repo> {
    #[instrument(skip_all)]
    pub async fn init(
        persistent_object: Arc<PersistentObject<Repo>>,
        meta_client_access_proxy: Arc<MetaClientAccessProxy>,
        server_dt: Arc<ServerDataTransfer>,
        gateway: Arc<SyncGateway<Repo>>
    ) -> anyhow::Result<VirtualDevice<Repo>> {
        info!("Run virtual device event handler");

        let virtual_device = Self {
            persistent_object,
            meta_client_proxy: meta_client_access_proxy.clone(),
            server_dt,
            gateway
        };

        Ok(virtual_device)
    }

    pub async fn run(&self) -> anyhow::Result<()> {

        loop {
            self.gateway.sync().await?;

            let p_vault = PersistentVault {
                p_obj: self.persistent_object.clone(),
            };

            let vault_status = p_vault.find_for_default_user().await?;
            match vault_status {
                VaultStatus::Outsider(_) => {
                    //nothing to do
                }
                VaultStatus::Member(vault) => {
                    //vault actions
                    let vault_log_desc = VaultDescriptor::VaultLog(vault.vault_name.clone())
                        .to_obj_desc();
                    let maybe_vault_log_event = self.persistent_object.find_tail_event(vault_log_desc).await?;
                    match maybe_vault_log_event {
                        None => {
                            //nothing to do
                        }
                        Some(vault_log_event) => {
                            let vault_log = VaultLogObject::try_from(vault_log_event)?;
                            if let VaultLogObject::Action(vault_action) = vault_log {
                                match vault_action.value {
                                    VaultAction::JoinRequest { candidate } => {
                                        let p_device_log = PersistentDeviceLog {
                                            p_obj: self.persistent_object.clone(),
                                        };

                                        p_device_log
                                            .accept_join_cluster_request(candidate)
                                            .await?;
                                    }
                                    VaultAction::UpdateMembership { .. } => {
                                        //changes made by another device, no need for any actions
                                    }
                                    VaultAction::AddMetaPassword { .. } => {
                                        //changes made by another device, no need for any actions
                                    }
                                }
                            };
                        }
                    }

                    // shared secret actions
                    let _p_ss_log = PersistentSharedSecret {
                        p_obj: self.persistent_object.clone(),
                    };

                    todo!("Implement SS log actions - replication request. This code must read the log and handle the events. Same as above for vault");
                }
            }

            self.gateway.sync().await?;

            async_std::task::sleep(std::time::Duration::from_millis(300))
                .await;
        }
    }
}
