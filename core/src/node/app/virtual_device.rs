use std::sync::Arc;
use log::warn;
use tracing::{info, instrument};

use crate::node::app::meta_app::meta_client_service::MetaClientAccessProxy;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::{VaultName, VaultStatus};
use crate::node::db::actions::sign_up_claim::SignUpClaim;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault_event::VaultAction;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
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
        info!("Initialize virtual device event handler");

        let virtual_device = Self {
            persistent_object,
            meta_client_proxy: meta_client_access_proxy.clone(),
            server_dt,
            gateway,
        };

        Ok(virtual_device)
    }

    #[instrument(skip_all)]
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Run virtual device event handler");

        let creds_repo = PersistentCredentials { p_obj: self.p_obj() };

        let device_name = DeviceName::generate();
        let user_creds = creds_repo
            .get_or_generate_user_creds(device_name, VaultName::test())
            .await?;

        self.gateway.sync().await?;

        //No matter what current vault status is, sign_up claim will handle the case properly
        info!("SignUp virtual device if needed");
        let sign_up_claim = SignUpClaim { p_obj: self.p_obj() };
        sign_up_claim.sign_up(user_creds.user()).await?;

        // Handle state changes
        loop {
            self.do_work(&user_creds).await?;
            async_std::task::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn do_work(&self, user_creds: &UserCredentials) -> anyhow::Result<()> {
        self.gateway.sync().await?;

        let p_vault = PersistentVault { p_obj: self.p_obj() };
        let vault_status = p_vault.find(user_creds.user()).await?;

        let VaultStatus::Member { member, vault } = vault_status else {
            warn!("Not a vault member");
            return Ok(());
        };

        let vault_name = vault.vault_name;
        //vault actions
        let vault_log_desc = VaultDescriptor::VaultLog(vault_name).to_obj_desc();

        let maybe_vault_log_event = self.persistent_object.find_tail_event(vault_log_desc).await?;

        if let Some(vault_log_event) = maybe_vault_log_event {
            let vault_log = vault_log_event.vault_log()?;

            if let VaultLogObject::Action(vault_action) = vault_log {
                match vault_action.value {
                    VaultAction::JoinClusterRequest { candidate } => {
                        let p_device_log = PersistentDeviceLog {
                            p_obj: self.persistent_object.clone(),
                        };

                        p_device_log.save_accept_join_request_event(member, candidate).await?;
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

        //todo!("Implement SS log actions - replication request. This code must read the log and handle the events. Same as above for vault");

        self.gateway.sync().await?;
        Ok(())
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.persistent_object.clone()
    }
}
