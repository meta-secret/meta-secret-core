use std::sync::Arc;

use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::messaging::{GenericAppStateRequest, SignUpRequest};
use crate::node::app::meta_app::meta_app_service::MetaClientAccessProxy;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::app_models::UserCredentials;
use crate::node::db::actions::join;
use crate::node::db::events::common::VaultInfo;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::read_db::read_db_service::ReadDbServiceProxy;
use crate::node::db::read_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::server_app::ServerDataTransfer;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Instrument};

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
        read_db_service_proxy: Arc<ReadDbServiceProxy>,
        server_dt: Arc<ServerDataTransfer>,
        gateway: Arc<SyncGateway<Repo>>,
        creds: UserCredentials
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
            creds: creds.clone()
        };

        let vault_name = "q";
        let device_name = "virtual-device";

        let sign_up_request = GenericAppStateRequest::SignUp(SignUpRequest {
            vault_name: String::from(vault_name),
            device_name: String::from(device_name),
        });

        //prepare for sign_up
        meta_client_access_proxy
            .send_request(sign_up_request.clone())
            .in_current_span()
            .await;

        let vault_info = meta_client
            .get_vault(String::from(vault_name))
            .in_current_span()
            .await;

        if let VaultInfo::Member { .. } = vault_info {
            //vd is already a member of a vault
        } else {
            //send a register request
            virtual_device
                .meta_client_proxy
                .send_request(sign_up_request.clone())
                .in_current_span()
                .await;
        }

        Ok(virtual_device)
    }

    pub async fn run(&self) {
        let vault_name = self.creds.user_sig.vault.name.clone();

        loop {
            self.gateway.sync().in_current_span().await;

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

                    let _ = vd_repo.save_event(accept_event).in_current_span().await;
                }

                let db_tail = self
                    .meta_client
                    .persistent_obj
                    .get_db_tail(vault_name.as_str())
                    .in_current_span()
                    .await
                    .unwrap();

                self.gateway
                    .sync_shared_secrets(&vault.vault_name, &self.creds, &db_tail)
                    .in_current_span()
                    .await;
            };

            async_std::task::sleep(std::time::Duration::from_millis(300))
                .in_current_span()
                .await;
        }
    }
}
