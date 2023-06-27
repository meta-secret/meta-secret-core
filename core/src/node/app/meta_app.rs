use std::error::Error;

use async_trait::async_trait;

use crate::models::DeviceInfo;
use crate::models::meta_vault::MetaVault;
use crate::models::user_credentials::UserCredentials;
use crate::node::db::generic_db::{FindOneQuery, KvLogEventRepo};
use crate::node::db::models::{GenericKvLogEvent, KvKey, KvLogEvent, ObjectCreator, ObjectDescriptor, ObjectId};

pub mod meta_vault_conf {
    pub const META_VAULT_KEY_NAME: &str = "main_meta_vault";
    pub const USER_CREDS_KEY_NAME: &str = "user_creds";
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

        let meta_vault_descriptor = ObjectDescriptor::MetaVault { name: vault_name };
        let key = KvKey::formation(&meta_vault_descriptor);
        let event: KvLogEvent<MetaVault> = KvLogEvent { key, value: meta_vault };

        let db_event = GenericKvLogEvent::MetaVault { event };

        let main_meta_vault_desc = ObjectDescriptor::MetaVault { name: meta_vault_conf::META_VAULT_KEY_NAME.to_string() };
        let main_meta_vault_obj_id = ObjectId::formation(&main_meta_vault_desc);
        self.save(&main_meta_vault_obj_id, &db_event).await?;

        Ok(())
    }

    async fn find_meta_vault(&self) -> Result<Option<MetaVault>, Err> {
        let meta_vault_desc = ObjectDescriptor::MetaVault { name: meta_vault_conf::META_VAULT_KEY_NAME.to_string() };
        let meta_vault_obj_id = ObjectId::formation(&meta_vault_desc);

        let maybe_meta_vault = self.find_one(&meta_vault_obj_id).await?;
        match maybe_meta_vault {
            None => Ok(None),
            Some(meta_vault) => match meta_vault {
                GenericKvLogEvent::MetaVault { event } => Ok(Some(event.value)),
                _ => {
                    panic!("Meta vault index: Invalid data")
                }
            },
        }
    }
}

#[async_trait(? Send)]
pub trait UserCredentialsManager<Err: Error> {
    async fn save_user_creds(&self, creds: UserCredentials) -> Result<(), Err>;
    async fn find_user_creds(&self) -> Result<Option<UserCredentials>, Err>;
}

#[async_trait(? Send)]
impl<T, Err> UserCredentialsManager<Err> for T
    where
        T: KvLogEventRepo<Err>,
        Err: Error,
{
    async fn find_user_creds(&self) -> Result<Option<UserCredentials>, Err> {
        let user_creds_desc = ObjectDescriptor::UserCreds { name: meta_vault_conf::USER_CREDS_KEY_NAME.to_string() };
        let obj_id = ObjectId::formation(&user_creds_desc);
        let maybe_creds = self.find_one(&obj_id).await?;
        match maybe_creds {
            None => Ok(None),
            Some(user_creds) => match user_creds {
                GenericKvLogEvent::UserCredentials { event } => Ok(Some(event.value)),
                _ => {
                    panic!("Meta vault index: Invalid data")
                }
            },
        }
    }

    async fn save_user_creds(&self, creds: UserCredentials) -> Result<(), Err> {
        let user_creds_desc = ObjectDescriptor::UserCreds { name: meta_vault_conf::USER_CREDS_KEY_NAME.to_string() };
        let event = KvLogEvent {
            key: KvKey::formation(&user_creds_desc),
            value: creds,
        };
        let generic_event = GenericKvLogEvent::UserCredentials { event };

        self.save_event(&generic_event).await
    }
}
