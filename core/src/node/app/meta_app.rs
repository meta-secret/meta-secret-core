use std::error::Error;

use async_trait::async_trait;

use crate::crypto::keys::KeyManager;
use crate::models::meta_vault::MetaVault;
use crate::models::user_credentials::UserCredentials;
use crate::models::DeviceInfo;
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::KvLogEventLocal;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::server::data_sync::MetaLogger;

#[async_trait(? Send)]
pub trait MetaVaultManager {
    async fn create_meta_vault(&self, vault_name: String, device_name: String) -> Result<MetaVault, Box<dyn Error>>;
    async fn find_meta_vault<L: MetaLogger>(&self, logger: &L) -> Result<Option<MetaVault>, Box<dyn Error>>;
}

#[async_trait(? Send)]
impl<T> MetaVaultManager for T
where
    T: KvLogEventRepo,
{
    async fn create_meta_vault(&self, vault_name: String, device_name: String) -> Result<MetaVault, Box<dyn Error>> {
        let device = DeviceInfo::from(device_name.to_string());
        let meta_vault = MetaVault {
            name: vault_name.to_string(),
            device: Box::new(device),
        };

        let key = KvKey::unit(&ObjectDescriptor::MetaVault);
        let event: KvLogEvent<MetaVault> = KvLogEvent {
            key,
            value: meta_vault.clone(),
        };

        let db_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::MetaVault { event: Box::new(event) });

        self.save(&ObjectId::meta_vault_index(), &db_event).await?;

        Ok(meta_vault)
    }

    async fn find_meta_vault<L: MetaLogger>(&self, logger: &L) -> Result<Option<MetaVault>, Box<dyn Error>> {
        logger.log("meta_app::find_meta_vault");

        let maybe_meta_vault = self.find_one(&ObjectId::meta_vault_index()).await?;

        match maybe_meta_vault {
            None => {
                logger.log("meta_app::find_meta_vault: meta vault not found");
                Ok(None)
            }
            Some(meta_vault) => match meta_vault {
                GenericKvLogEvent::LocalEvent(KvLogEventLocal::MetaVault { event }) => Ok(Some(event.value)),

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
pub trait UserCredentialsManager: KvLogEventRepo {
    async fn save_user_creds(&self, creds: &UserCredentials) -> Result<(), Box<dyn Error>>;
    async fn find_user_creds(&self) -> Result<Option<UserCredentials>, Box<dyn Error>>;
    async fn generate_user_creds(&self, vault_name: String, device_name: String) -> UserCredentials;
    async fn get_or_generate_user_creds(&self, vault_name: String, device_name: String) -> UserCredentials;
}

#[async_trait(? Send)]
impl<T> UserCredentialsManager for T
where
    T: KvLogEventRepo,
{
    async fn find_user_creds(&self) -> Result<Option<UserCredentials>, Box<dyn Error>> {
        let obj_id = ObjectId::unit(&ObjectDescriptor::UserCreds);
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

    async fn save_user_creds(&self, creds: &UserCredentials) -> Result<(), Box<dyn Error>> {
        let event = KvLogEvent {
            key: KvKey::unit(&ObjectDescriptor::UserCreds),
            value: creds.clone(),
        };
        let generic_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::UserCredentials { event: Box::new(event) });

        self.save_event(&generic_event).await
    }

    async fn generate_user_creds(&self, vault_name: String, device_name: String) -> UserCredentials {
        let meta_vault = self.create_meta_vault(vault_name, device_name).await.unwrap();

        let security_box = KeyManager::generate_security_box(meta_vault.name);
        let user_sig = security_box.get_user_sig(&meta_vault.device);
        let creds = UserCredentials::new(security_box, user_sig);

        self.save_user_creds(&creds).await.unwrap();

        creds
    }

    async fn get_or_generate_user_creds(&self, vault_name: String, device_name: String) -> UserCredentials {
        let server_creds_result = self.find_user_creds().await;

        match server_creds_result {
            Ok(maybe_creds) => match maybe_creds {
                None => self.generate_user_creds(vault_name, device_name).await,
                Some(creds) => creds,
            },
            Err(_) => self.generate_user_creds(vault_name, device_name).await,
        }
    }
}
