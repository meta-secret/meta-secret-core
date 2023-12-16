use std::sync::Arc;

use crate::node::app::meta_client::MetaClient;
use crate::node::app::meta_app::meta_client_service::MetaClientAccessProxy;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::db::actions::join;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::server_app::ServerDataTransfer;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Instrument};
use crate::node::common::model::user::UserCredentials;

pub struct VirtualDevice<Repo: KvLogEventRepo> {
    pub meta_client: Arc<MetaClient<Repo>>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub server_dt: Arc<ServerDataTransfer>,
    gateway: Arc<SyncGateway<Repo>>,
    creds: UserCredentials,
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
        gateway: Arc<SyncGateway<Repo>>,
        creds: UserCredentials
    ) -> anyhow::Result<VirtualDevice<Repo>> {
        info!("Run virtual device event handler");

        let meta_client = Arc::new(MetaClient {
            persistent_obj: persistent_object.clone(),
            sync_gateway: gateway.clone()
        });

        let virtual_device = Self {
            meta_client: meta_client.clone(),
            meta_client_proxy: meta_client_access_proxy.clone(),
            server_dt,
            gateway,
            creds
        };

        Ok(virtual_device)
    }

    pub async fn run(&self) {
        let user_creds = &self.creds;

        loop {
             self.gateway.sync().await?;

            // read log messages and take actions accordingly

            if let VaultStore::Store { tail_id, vault, .. } = vault_store {
                let vd_repo = self.meta_client.persistent_obj.repo.clone();

                let latest_event = vd_repo.find_one(tail_id).await;

                if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                    let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                        event: join::accept_join_request(&event, &vault),
                    });

                    let _ = vd_repo.save(accept_event).in_current_span().await;
                }

                let db_tail = self
                    .meta_client
                    .persistent_obj
                    .get_db_tail(vault_name.as_str())
                    .in_current_span()
                    .await
                    .unwrap();

                self.gateway
                    .sync_shared_secrets(&vault.vault_name, &self.device_creds, &db_tail)
                    .in_current_span()
                    .await;
            };

            async_std::task::sleep(std::time::Duration::from_millis(300))
                .await;
        }
    }
}
