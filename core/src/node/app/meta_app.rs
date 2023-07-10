use std::error::Error;

use async_trait::async_trait;

use crate::models::DeviceInfo;
use crate::models::meta_vault::MetaVault;
use crate::models::user_credentials::UserCredentials;
use crate::node::db::generic_db::{KvLogEventRepo};
use crate::node::db::models::{GenericKvLogEvent, KvLogEventLocal, KvKey, KvLogEvent, ObjectCreator, ObjectDescriptor};
use crate::node::db::events::object_id::ObjectId;
use crate::node::server::meta_server::MetaLogger;

#[async_trait(? Send)]
pub trait MetaVaultManager<Err: Error> {
    async fn create_meta_vault<L: MetaLogger>(&self, vault_name: String, device_name: String, logger: &L) -> Result<MetaVault, Err>;
    async fn find_meta_vault<L: MetaLogger>(&self, logger: &L) -> Result<Option<MetaVault>, Err>;
}

#[async_trait(? Send)]
impl<T, Err> MetaVaultManager<Err> for T
    where
        T: KvLogEventRepo<Err>,
        Err: Error,
{
    async fn create_meta_vault<L: MetaLogger>(&self, vault_name: String, device_name: String, logger: &L) -> Result<MetaVault, Err> {
        logger.log("meta_app::create_meta_vault");

        let device = DeviceInfo::from(device_name.to_string());
        let meta_vault = MetaVault {
            name: vault_name.to_string(),
            device: Box::new(device),
        };

        let meta_vault_descriptor = ObjectDescriptor::MetaVault;
        let key = KvKey::unit(&meta_vault_descriptor);
        let event: KvLogEvent<MetaVault> = KvLogEvent { key, value: meta_vault.clone() };

        let db_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::MetaVault { event });

        self.save(&ObjectId::meta_vault_index(), &db_event).await?;

        Ok(meta_vault)
    }

    async fn find_meta_vault<L: MetaLogger>(&self, logger: &L) -> Result<Option<MetaVault>, Err> {
        logger.log("meta_app::find_meta_vault");

        let maybe_meta_vault = self.find_one(&ObjectId::meta_vault_index()).await?;

        match maybe_meta_vault {
            None => {
                logger.log("meta_app::find_meta_vault: meta vault not found");
                Ok(None)
            },
            Some(meta_vault) => match meta_vault {
                GenericKvLogEvent::LocalEvent(KvLogEventLocal::MetaVault { event }) => {
                    Ok(Some(event.value))
                }

                _ => {
                    let err_msg = "Meta vault index: Invalid data";
                    logger.log(err_msg);
                    panic!("{}", err_msg)
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
        let user_creds_desc = ObjectDescriptor::UserCreds;
        let obj_id = ObjectId::unit(&user_creds_desc);
        let maybe_creds = self.find_one(&obj_id).await?;
        match maybe_creds {
            None => Ok(None),
            Some(user_creds) => match user_creds {
                GenericKvLogEvent::LocalEvent(KvLogEventLocal::UserCredentials { event }) => Ok(Some(event.value)),
                _ => {
                    panic!("Meta vault index: Invalid data")
                }
            },
        }
    }

    async fn save_user_creds(&self, creds: UserCredentials) -> Result<(), Err> {
        let user_creds_desc = ObjectDescriptor::UserCreds;
        let event = KvLogEvent {
            key: KvKey::unit(&user_creds_desc),
            value: creds,
        };
        let generic_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::UserCredentials { event });

        self.save_event(&generic_event).await
    }
}
