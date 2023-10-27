use std::sync::Arc;

use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::messaging::{GenericAppStateRequest, SignUpRequest};
use crate::node::app::meta_app::meta_client_service::MetaClientAccessProxy;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::db::actions::join;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::read_db::read_db_service::ReadDbServiceProxy;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::server_app::ServerDataTransfer;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Instrument};
use crate::node::common::model::device::DeviceCredentials;

pub struct VirtualDevice<Repo: KvLogEventRepo> {
    pub meta_client: Arc<MetaClient<Repo>>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub server_dt: Arc<ServerDataTransfer>,
    gateway: Arc<SyncGateway<Repo>>,
    device_creds: DeviceCredentials,
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
        read_db_service_proxy: Arc<ReadDbServiceProxy>,
        server_dt: Arc<ServerDataTransfer>,
        gateway: Arc<SyncGateway<Repo>>,
        creds: DeviceCredentials
    ) -> anyhow::Result<VirtualDevice<Repo>> {
        info!("Run virtual device event handler");

        let meta_client = Arc::new(MetaClient {
            persistent_obj: persistent_object.clone(),
            read_db_service_proxy: read_db_service_proxy.clone(),
        });

        let virtual_device = Self {
            meta_client: meta_client.clone(),
            meta_client_proxy: meta_client_access_proxy.clone(),
            server_dt,
            gateway,
            device_creds: creds.clone()
        };

        persistent_object.global_index.

        Ok(virtual_device)
    }

    pub async fn run(&self) {
        //let vault_name = self.creds.user_sig.vault.name.clone();

        loop {
            self.gateway.sync().in_current_span().await?;

            let read_db_service = self.meta_client.read_db_service_proxy.clone();
            let vault_store = read_db_service
                .get_vault_store(vault_name.clone())
                .in_current_span()
                .await
                .unwrap();

            if let VaultStore::Store { tail_id, vault, .. } = vault_store {
                let vd_repo = self.meta_client.persistent_obj.repo.clone();

                let latest_event = vd_repo.find_one(tail_id).in_current_span().await;

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
                .in_current_span()
                .await;
        }
    }
}
