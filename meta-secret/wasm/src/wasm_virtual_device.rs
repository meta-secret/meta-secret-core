use std::sync::Arc;
use tracing::{info, instrument, Instrument};
use wasm_bindgen_futures::spawn_local;
use meta_secret_core::node::app::meta_app::meta_client_service::{MetaClientAccessProxy, MetaClientDataTransfer, MetaClientService, MetaClientStateProvider};
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::virtual_device::VirtualDevice;
use meta_secret_core::node::common::data_transfer::MpscDataTransfer;
use meta_secret_core::node::common::meta_tracing::vd_span;
use meta_secret_core::node::common::model::device::common::DeviceName;
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_secret_core::node::db::objects::persistent_object::PersistentObject;
use meta_secret_core::node::db::repo::generic_db::KvLogEventRepo;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::wasm_repo::WasmSyncProtocol;

#[instrument(name = "Vd", skip_all)]
pub async fn virtual_device_setup<Repo: KvLogEventRepo>(
    device_repo: Arc<Repo>,
    sync_protocol: Arc<WasmSyncProtocol<Repo>>,
) -> anyhow::Result<()> {
    info!("virtual device initialization");

    let persistent_object = Arc::new(PersistentObject::new(device_repo.clone()));

    let creds_repo = PersistentCredentials {
        p_obj: persistent_object.clone(),
    };

    let user_creds = creds_repo
        .get_or_generate_user_creds(DeviceName::virtual_device(), VaultName::test())
        .await?;
    let device_creds = Arc::new(user_creds.device_creds.clone());

    let dt_meta_client = Arc::new(MetaClientDataTransfer {
        dt: MpscDataTransfer::new(),
    });

    let gateway = Arc::new(SyncGateway {
        id: String::from("vd-gateway"),
        p_obj: persistent_object.clone(),
        sync: sync_protocol.clone(),
        device_creds: device_creds.clone(),
    });

    let state_provider = Arc::new(MetaClientStateProvider::new());
    let meta_client_service = MetaClientService {
        data_transfer: dt_meta_client.clone(),
        sync_gateway: gateway.clone(),
        state_provider,
        p_obj: persistent_object.clone(),
        device_creds: device_creds.clone(),
    };

    spawn_local(async move {
        meta_client_service
            .run()
            .instrument(vd_span())
            .await
            .unwrap();
    });

    let meta_client_access_proxy = Arc::new(MetaClientAccessProxy { dt: dt_meta_client });
    let vd = VirtualDevice::init(persistent_object, meta_client_access_proxy, gateway).await?;
    let vd = Arc::new(vd);
    spawn_local(async move { vd.run().instrument(vd_span()).await.unwrap() });

    Ok(())
}

