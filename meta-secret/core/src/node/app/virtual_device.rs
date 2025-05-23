use std::sync::Arc;
use tracing::{info, instrument};

use crate::node::app::meta_app::meta_client_service::MetaClientAccessProxy;
use crate::node::app::orchestrator::MetaOrchestrator;
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::app::sync::sync_protocol::SyncProtocol;
use crate::node::common::model::device::common::DeviceName;
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::actions::sign_up::claim::SignUpClaim;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;
use crate::crypto::keys::TransportSk;

pub struct VirtualDevice<Repo: KvLogEventRepo, Sync: SyncProtocol> {
    p_obj: Arc<PersistentObject<Repo>>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    gateway: Arc<SyncGateway<Repo, Sync>>,
    master_key: TransportSk,
}

impl<Repo: KvLogEventRepo, Sync: SyncProtocol> VirtualDevice<Repo, Sync> {
    #[instrument(skip_all)]
    pub async fn init(
        persistent_object: Arc<PersistentObject<Repo>>,
        meta_client_access_proxy: Arc<MetaClientAccessProxy>,
        gateway: Arc<SyncGateway<Repo, Sync>>,
        master_key: TransportSk
    ) -> Result<VirtualDevice<Repo, Sync>> {
        info!("Initialize virtual device event handler");

        let virtual_device = Self {
            p_obj: persistent_object,
            meta_client_proxy: meta_client_access_proxy.clone(),
            gateway,
            master_key
        };

        Ok(virtual_device)
    }

    #[instrument(skip_all)]
    pub async fn run(&self) -> Result<()> {
        info!("Run virtual device event handler");

        let device_name = DeviceName::virtual_device();
        //No matter what current vault status is, sign_up claim will handle the case properly
        info!("SignUp virtual device if needed");
        let sign_up_claim = SignUpClaim {
            p_obj: self.p_obj(),
        };
        let user_creds = sign_up_claim
            .prepare_sign_up(device_name, VaultName::test(), self.master_key.clone())
            .await?;
        self.gateway.sync(user_creds.user()).await?;
        sign_up_claim.sign_up(user_creds.user()).await?;

        // Handle state changes
        loop {
            self.do_work(&user_creds).await?;
            async_std::task::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn do_work(&self, user_creds: &UserCreds) -> Result<()> {
        self.gateway.sync(user_creds.user()).await?;

        let orchestrator = MetaOrchestrator {
            p_obj: self.p_obj(),
            user_creds: user_creds.clone(),
        };

        orchestrator.orchestrate().await?;

        self.gateway.sync(user_creds.user()).await?;
        Ok(())
    }

    fn p_obj(&self) -> Arc<PersistentObject<Repo>> {
        self.p_obj.clone()
    }
}
