use std::sync::Arc;

use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::messaging::{GenericAppStateRequest, SignUpRequest};
use crate::node::app::meta_app::meta_app_service::MetaClientAccessProxy;
use crate::node::app::meta_vault_manager::UserCredentialsManager;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::db::actions::join;
use crate::node::db::events::common::VaultInfo;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbServiceProxy;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::server::server_app::ServerDataTransfer;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Instrument};

pub struct VirtualDevice<Repo: KvLogEventRepo> {
    pub meta_client: Arc<MetaClient<Repo>>,
    pub meta_client_proxy: Arc<MetaClientAccessProxy>,
    pub server_dt: Arc<ServerDataTransfer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp,
}

impl<Repo: KvLogEventRepo> VirtualDevice<Repo> {
    pub fn new(
        persistent_object: Arc<PersistentObject<Repo>>,
        meta_client_access_proxy: Arc<MetaClientAccessProxy>,
        meta_db_service_proxy: Arc<MetaDbServiceProxy>,
        dt: Arc<ServerDataTransfer>,
    ) -> VirtualDevice<Repo> {
        Self {
            meta_client: Arc::new(MetaClient {
                persistent_obj: persistent_object,
                meta_db_service_proxy: meta_db_service_proxy.clone(),
            }),
            meta_client_proxy: meta_client_access_proxy,
            server_dt: dt,
        }
    }

    #[instrument(skip_all)]
    pub async fn event_handler(
        persistent_object: Arc<PersistentObject<Repo>>,
        meta_client_access_proxy: Arc<MetaClientAccessProxy>,
        meta_db_service_proxy: Arc<MetaDbServiceProxy>,
        dt: Arc<ServerDataTransfer>,
        gateway: Arc<SyncGateway<Repo>>
    ) {
        info!("Run virtual device event handler");

        let virtual_device = {
            let vd = VirtualDevice::new(
                persistent_object.clone(),
                meta_client_access_proxy.clone(),
                meta_db_service_proxy.clone(),
                dt.clone(),
            );
            Arc::new(vd)
        };

        info!("Generate device creds");
        let creds = persistent_object
            .repo
            .get_or_generate_user_creds(String::from("q"), String::from("virtual-device"))
            .in_current_span()
            .await;

        let vault_name = "q";
        let device_name = "virtual-device";

        let sign_up_request = GenericAppStateRequest::SignUp(SignUpRequest {
            vault_name: String::from(vault_name),
            device_name: String::from(device_name),
        });

        //prepare for sign_up
        virtual_device
            .meta_client_proxy
            .send_request(sign_up_request.clone())
            .in_current_span()
            .await;

        let vault_info = virtual_device.meta_client.get_vault(&creds).in_current_span().await;
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

        loop {
            gateway.sync().in_current_span().await;

            let meta_db_service = virtual_device.meta_client.meta_db_service_proxy.clone();
            meta_db_service.sync_db().in_current_span().await;

            meta_db_service
                .update_with_vault(creds.user_sig.vault.name.clone())
                .in_current_span()
                .await;

            let vault_store = meta_db_service.get_vault_store().in_current_span().await.unwrap();

            if let VaultStore::Store { tail_id, vault, .. } = vault_store {
                let vd_repo = virtual_device.meta_client.persistent_obj.repo.clone();

                let latest_event = vd_repo.find_one(tail_id).in_current_span().await;

                if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                    let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                        event: join::accept_join_request(&event, &vault),
                    });

                    let _ = vd_repo.save_event(accept_event).in_current_span().await;
                }

                gateway.send_shared_secrets(&vault).in_current_span().await;
            };

            async_std::task::sleep(std::time::Duration::from_millis(300))
                .in_current_span()
                .await;
        }
    }
}
