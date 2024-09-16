use std::sync::Arc;
use anyhow::bail;
use log::error;
use tracing::{info, instrument};

use crate::node::app::meta_app::meta_client_service::MetaClientAccessProxy;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::model::device::DeviceName;
use crate::node::common::model::user::{UserDataOutsider, UserDataOutsiderStatus};
use crate::node::common::model::vault::{VaultName, VaultStatus};
use crate::node::db::actions::sign_up_claim::SignUpClaim;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::local_event::CredentialsObject;
use crate::node::db::events::vault_event::{VaultAction, VaultLogObject};
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::server_app::ServerDataTransfer;

pub struct VirtualDevice<Repo: KvLogEventRepo> {
    persistent_object: Arc<PersistentObject<Repo>>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub server_dt: Arc<ServerDataTransfer>,
    gateway: Arc<SyncGateway<Repo>>,
}

impl<Repo: KvLogEventRepo> VirtualDevice<Repo> {
    #[instrument(skip_all)]
    pub async fn init(
        persistent_object: Arc<PersistentObject<Repo>>,
        meta_client_access_proxy: Arc<MetaClientAccessProxy>,
        server_dt: Arc<ServerDataTransfer>,
        gateway: Arc<SyncGateway<Repo>>,
    ) -> anyhow::Result<VirtualDevice<Repo>> {
        info!("Run virtual device event handler");

        let virtual_device = Self {
            persistent_object,
            meta_client_proxy: meta_client_access_proxy.clone(),
            server_dt,
            gateway,
        };

        Ok(virtual_device)
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        self.gateway.sync().await?;

        let creds_repo = CredentialsRepo { p_obj: self.p_obj() };

        let maybe_creds_obj = creds_repo.find().await?;

        let device_creds = match maybe_creds_obj {
            None => creds_repo.generate_device_creds(DeviceName::generate()).await?,
            Some(creds_obj) => {
                match creds_obj {
                    CredentialsObject::Device(device_event) => device_event.value,
                    CredentialsObject::DefaultUser(user_event) => user_event.value.device_creds
                }
            }
        };

        let device_name = device_creds.device.name;
        let _ = creds_repo
            .get_or_generate_user_creds(device_name, VaultName::from("q"))
            .await?;

        loop {
            self.do_work().await?;
            async_std::task::sleep(std::time::Duration::from_millis(300)).await;
        }
    }

    async fn do_work(&self) -> anyhow::Result<()> {
        let p_vault = PersistentVault { p_obj: self.p_obj() };
        self.gateway.sync().await?;

        let maybe_vault_status = p_vault.find_for_default_user().await?;
        let Some(vault_status) = maybe_vault_status else {
            bail!("User credentials not found")
        };

        match vault_status {
            VaultStatus::Outsider(UserDataOutsider { user_data, status }) => {
                match status {
                    UserDataOutsiderStatus::NonMember => {
                        //check if join request has been sent, if not then send it, if yes, skip
                        let sign_up = SignUpClaim {p_obj: self.p_obj()};
                    }
                    UserDataOutsiderStatus::Pending => {}
                    UserDataOutsiderStatus::Declined => {}
                }
            }
            VaultStatus::Member { member, vault } => {
                let vault_name = vault.vault_name;
                //vault actions
                let vault_log_desc = VaultDescriptor::VaultLog(vault_name)
                    .to_obj_desc();

                let maybe_vault_log_event = self.persistent_object
                    .find_tail_event(vault_log_desc)
                    .await?;

                if let Some(vault_log_event) = maybe_vault_log_event {
                    let vault_log = vault_log_event.vault_log()?;

                    if let VaultLogObject::Action(vault_action) = vault_log {
                        match vault_action.value {
                            VaultAction::JoinClusterRequest { candidate } => {
                                let p_device_log = PersistentDeviceLog {
                                    p_obj: self.persistent_object.clone(),
                                };

                                p_device_log
                                    .save_accept_join_request_event(member, candidate)
                                    .await?;
                            }
                            VaultAction::UpdateMembership { .. } => {
                                //changes made by another device, no need for any actions
                            }
                            VaultAction::AddMetaPassword { .. } => {
                                //changes made by another device, no need for any actions
                            }
                            VaultAction::CreateVault(_) => {
                                // server's responsibities
                            }
                        }
                    };
                }

                // shared secret actions
                let _p_ss_log = PersistentSharedSecret {
                    p_obj: self.persistent_object.clone(),
                };

                todo!("Implement SS log actions - replication request. This code must read the log and handle the events. Same as above for vault");
            }
        }

        self.gateway.sync().await?;
        Ok(())
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.persistent_object.clone()
    }
}
