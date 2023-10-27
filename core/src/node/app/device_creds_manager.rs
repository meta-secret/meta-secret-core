use anyhow::anyhow;
use async_trait::async_trait;
use tracing::{instrument, Instrument};

use crate::crypto::keys::{KeyManager, OpenBox};
use crate::node::common::model::device::{DeviceCredentials, DeviceData, DeviceId};
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, UnitEvent};
use crate::node::db::events::local::DeviceCredentialsObject;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;

#[async_trait(? Send)]
pub trait DeviceCredentialsManager {
    async fn save_device_creds(&self, creds: DeviceCredentials) -> anyhow::Result<ObjectId>;
    async fn find_device_creds(&self) -> anyhow::Result<Option<DeviceCredentials>>;
    async fn generate_device_creds(&self, device_name: String) -> anyhow::Result<DeviceCredentials>;
    async fn get_or_generate_device_creds(&self, device_name: String) -> anyhow::Result<DeviceCredentials>;
}

#[async_trait(? Send)]
impl<T: KvLogEventRepo> DeviceCredentialsManager for T {
    async fn save_device_creds(&self, creds: DeviceCredentials) -> anyhow::Result<ObjectId> {
        let generic_event = {
            let creds_obj = DeviceCredentialsObject::unit(creds.clone());
            GenericKvLogEvent::Credentials(creds_obj)
        };

        self.save(generic_event).await
    }

    #[instrument(skip_all)]
    async fn find_device_creds(&self) -> anyhow::Result<Option<DeviceCredentials>> {
        let obj_id = ObjectId::unit(ObjectDescriptor::DeviceCredsIndex);
        let maybe_creds = self.find_one(obj_id).await?;

        if let None = maybe_creds {
            return Ok(None);
        }

        if let Some(GenericKvLogEvent::Credentials(DeviceCredentialsObject { event })) = maybe_creds {
            return Ok(Some(event.value))
        } else {
            Err(anyhow!("Meta vault index: Invalid event type"))
        }
    }

    async fn generate_device_creds(&self, device_name: String) -> anyhow::Result<DeviceCredentials> {
        let secret_box = KeyManager::generate_secret_box();
        let open_box = OpenBox::from(&secret_box);
        let device_data = DeviceData::from(device_name, open_box);

        let creds = DeviceCredentials {
            secret_box,
            device: device_data
        };

        self.save_device_creds(creds.clone()).await?;

        Ok(creds)
    }

    async fn get_or_generate_device_creds(&self, device_name: String) -> anyhow::Result<DeviceCredentials> {
        let maybe_creds = self.find_device_creds().await?;

        match maybe_creds {
            None => self.generate_device_creds(device_name).await,
            Some(creds) => Ok(creds),
        }
    }
}
