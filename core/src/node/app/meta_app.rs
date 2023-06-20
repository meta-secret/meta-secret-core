use std::error::Error;

use async_trait::async_trait;

use crate::models::meta_vault::MetaVault;
use crate::models::DeviceInfo;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::models::{GenericKvLogEvent, KvKey, KvLogEvent, ObjectCreator, ObjectDescriptor, ObjectId};

pub mod meta_vault_conf {
    pub const KEY_NAME: &str = "main_meta_vault";
}

#[async_trait(? Send)]
pub trait MetaVaultManager<Err: Error> {
    async fn create_meta_vault(&self, vault_name: String, device_name: String) -> Result<(), Err>;
    async fn find_meta_vault(&self) -> Result<Option<MetaVault>, Err>;
}

#[async_trait(? Send)]
impl<T, Err> MetaVaultManager<Err> for T
where
    T: KvLogEventRepo<Err>,
    Err: Error,
{
    async fn create_meta_vault(&self, vault_name: String, device_name: String) -> Result<(), Err> {
        let device = DeviceInfo::from(device_name.to_string());
        let meta_vault = MetaVault {
            name: vault_name.to_string(),
            device: Box::new(device),
        };
        let meta_vault_descriptor = ObjectDescriptor::meta_vault(meta_vault_conf::KEY_NAME);
        let key = KvKey::formation(&meta_vault_descriptor);
        let event: KvLogEvent<MetaVault> = KvLogEvent { key, value: meta_vault };

        let db_event = GenericKvLogEvent::MetaVault { event };

        self.save(&db_event).await
    }

    async fn find_meta_vault(&self) -> Result<Option<MetaVault>, Err> {
        let meta_vault_descriptor = ObjectDescriptor::meta_vault(meta_vault_conf::KEY_NAME);
        let obj_id = ObjectId::formation(&meta_vault_descriptor);

        let maybe_event = self.find_one(obj_id.id.as_str()).await?;

        if let Some(GenericKvLogEvent::MetaVault { event }) = maybe_event {
            Ok(Some(event.value))
        } else {
            Ok(None)
        }
    }
}
