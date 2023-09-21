use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::node::app::app_state_manager::JsAppStateManager;
use crate::node::app::client_meta_app::MetaClient;
use crate::node::app::meta_app::messaging::{GenericAppStateRequest, SignUpRequest};
use crate::node::app::meta_app::meta_app_service::MetaClientService;
use crate::node::app::meta_vault_manager::UserCredentialsManager;
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::db::actions::join;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::vault_event::VaultObject;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_service::MetaDbService;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;
use crate::node::server::data_sync::DataSyncMessage;

pub struct VirtualDevice<Repo: KvLogEventRepo, Logger: MetaLogger, State: JsAppStateManager> {
    pub meta_client: Arc<MetaClient<Repo, Logger>>,
    pub meta_client_service: Arc<MetaClientService<Repo, Logger, State>>,
    pub data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
    pub logger: Arc<Logger>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VirtualDeviceEvent {
    Init,
    SignUp,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger, State: JsAppStateManager> VirtualDevice<Repo, Logger, State> {
    pub fn new(
        persistent_object: Arc<PersistentObject<Repo, Logger>>,
        meta_client_service: Arc<MetaClientService<Repo, Logger, State>>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        logger: Arc<Logger>,
    ) -> VirtualDevice<Repo, Logger, State> {
        Self {
            meta_client: Arc::new(MetaClient {
                logger: logger.clone(),
                persistent_obj: persistent_object,
                meta_db_service,
            }),
            meta_client_service,
            data_transfer,
            logger: logger.clone(),
        }
    }

    pub async fn event_handler(
        persistent_object: Arc<PersistentObject<Repo, Logger>>,
        meta_client_service: Arc<MetaClientService<Repo, Logger, State>>,
        meta_db_service: Arc<MetaDbService<Repo, Logger>>,
        data_transfer: Arc<MpscDataTransfer<DataSyncMessage, Vec<GenericKvLogEvent>>>,
        logger: Arc<Logger>,
    ) {
        logger.info("Run virtual device event handler");

        let virtual_device = {
            let vd = VirtualDevice::<Repo, Logger, State>::new(
                persistent_object.clone(),
                meta_client_service,
                meta_db_service.clone(),
                data_transfer.clone(),
                logger.clone(),
            );
            Arc::new(vd)
        };

        logger.info("Generate device creds");
        let creds = persistent_object
            .repo
            .get_or_generate_user_creds(String::from("q"), String::from("virtual-device"))
            .await;

        let gateway = SyncGateway::new(
            persistent_object.repo.clone(),
            meta_db_service.clone(),
            data_transfer.clone(),
            String::from("vd-gateway"),
            logger.clone(),
        );

        let vault_name = "q";
        let device_name = "virtual-device";

        let sign_up_request = GenericAppStateRequest::SignUp(SignUpRequest {
            vault_name: String::from(vault_name),
            device_name: String::from(device_name),
        });

        virtual_device.meta_client_service.send_request(sign_up_request).await;

        loop {
            gateway.sync().await;
            meta_db_service.sync_db().await;

            meta_db_service
                .update_with_vault(creds.user_sig.vault.name.clone())
                .await;

            let vault_store = meta_db_service.get_vault_store().await.unwrap();

            if let VaultStore::Store { tail_id, vault, .. } = vault_store {
                let latest_event = virtual_device.meta_client.persistent_obj.repo.find_one(&tail_id).await;

                if let Ok(Some(GenericKvLogEvent::Vault(VaultObject::JoinRequest { event }))) = latest_event {
                    let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                        event: join::accept_join_request(&event, &vault),
                    });

                    let _ = virtual_device
                        .meta_client
                        .persistent_obj
                        .repo
                        .save_event(&accept_event)
                        .await;
                }

                gateway.send_shared_secrets(&vault).await;
            };

            async_std::task::sleep(std::time::Duration::from_millis(300)).await;
        }
    }
}
