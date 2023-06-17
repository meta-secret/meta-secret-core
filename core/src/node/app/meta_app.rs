use async_trait::async_trait;
use std::error::Error;

use crate::models::meta_vault::MetaVault;
use crate::models::DeviceInfo;
use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo, SaveCommand};
use crate::node::db::models::{
    AppOperation, AppOperationType, KvKey, KvLogEvent, KvValueType, ObjectCreator, ObjectDescriptor,
};

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
        let meta_vault_descriptor = ObjectDescriptor::meta_vault(vault_name.as_str());
        let key = KvKey::formation(&meta_vault_descriptor);
        let event = KvLogEvent {
            key,
            cmd_type: AppOperationType::Update(AppOperation::MetaVault),
            val_type: KvValueType::MetaVault,
            value: serde_json::to_value(&meta_vault).unwrap(),
        };

        self.save(&event).await
    }

    async fn find_meta_vault(&self) -> Result<Option<MetaVault>, Err> {
        let maybe_event = self.find_one(meta_vault_conf::KEY_NAME).await?;
        let event_js = maybe_event.map(|evt| serde_json::from_value(evt.value).unwrap());
        Ok(event_js)
    }
}
