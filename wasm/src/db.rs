use wasm_bindgen::JsValue;
use web_sys::DomException;
use meta_secret_core::node::app::meta_app::PersistentMetaVault;
use crate::db::meta_vault::MetaVaultWasmRepo;

pub const DB_NAME: &str = "meta_secret_db";

#[derive(thiserror::Error, Debug)]
pub enum WasmDbError {
    #[error("IndexedDb error")]
    JsIndexedDbError(DomException),

    #[error(transparent)]
    WasmBindGenError {
        #[from]
        source: serde_wasm_bindgen::Error,
    },

    #[error("JsValue error")]
    JsValueError(JsValue),

    #[error("Db error: {0}")]
    DbCustomError(String),
}

pub mod user_credentials {
    use async_trait::async_trait;
    use meta_secret_core::models::UserCredentials;
    use meta_secret_core::node::db::generic_db::{FindOneQuery, SaveCommand, UserCredentialsRepo};

    use crate::db::{WasmDbError, DB_NAME};
    use crate::{idbGet, idbSave};

    pub mod store_conf {
        pub const STORE_NAME: &str = "meta_vault";
        pub const KEY_NAME: &str = "creds";
    }

    pub struct UserCredentialsWasmRepo {}

    impl UserCredentialsWasmRepo {
        pub async fn find_user_credentials(&self) -> Result<Option<UserCredentials>, WasmDbError> {
            self.find_one(store_conf::KEY_NAME).await
        }
    }

    #[async_trait(? Send)]
    impl SaveCommand<UserCredentials, WasmDbError> for UserCredentialsWasmRepo {

        async fn save(&self, key: &str, creds: &UserCredentials) -> Result<(), Self::Error> {
            let creds_js = serde_wasm_bindgen::to_value(creds)?;
            idbSave(DB_NAME, store_conf::STORE_NAME, key, creds_js).await;
            Ok(())
        }
    }

    #[async_trait(? Send)]
    impl FindOneQuery<UserCredentials, WasmDbError> for UserCredentialsWasmRepo {

        async fn find_one(&self, key: &str) -> Result<Option<UserCredentials>, Self::Error> {
            let creds_js = idbGet(DB_NAME, store_conf::STORE_NAME, key).await;
            let creds: UserCredentials = serde_wasm_bindgen::from_value(creds_js)?;
            Ok(Some(creds))
        }
    }

    #[async_trait(? Send)]
    impl UserCredentialsRepo for UserCredentialsWasmRepo {}
}

pub mod meta_vault {
    use async_trait::async_trait;
    use meta_secret_core::models::MetaVault;
    use meta_secret_core::node::db::generic_db::{FindOneQuery, MetaVaultRepo, SaveCommand};

    use crate::db::WasmDbError;
    use crate::db::DB_NAME;
    use crate::{idbGet, idbSave};

    pub mod store_conf {
        pub const STORE_NAME: &str = "meta_vault";
        pub const KEY_NAME: &str = "vault";
    }

    pub struct MetaVaultWasmRepo {}

    #[async_trait(? Send)]
    impl SaveCommand<MetaVault, WasmDbError> for MetaVaultWasmRepo {

        async fn save(&self, key: &str, vault: &MetaVault) -> Result<(), Self::Error> {
            let vault_js = serde_wasm_bindgen::to_value(vault)?;
            idbSave(DB_NAME, store_conf::STORE_NAME, key, vault_js).await;
            Ok(())
        }
    }

    #[async_trait(? Send)]
    impl FindOneQuery<MetaVault, WasmDbError> for MetaVaultWasmRepo {
        async fn find_one(&self, key: &str) -> Result<Option<MetaVault>, Self::Error> {
            let vault_js = idbGet(DB_NAME, store_conf::STORE_NAME, key).await;
            if vault_js.is_undefined() {
                Ok(None)
            } else {
                let vault = serde_wasm_bindgen::from_value(vault_js)?;
                Ok(Some(vault))
            }
        }
    }

    impl MetaVaultWasmRepo {
        pub async fn find_meta_vault(&self) -> Result<Option<MetaVault>, WasmDbError> {
            self.find_one(store_conf::KEY_NAME).await
        }
    }
}

pub mod meta_pass {
    use async_trait::async_trait;
    use meta_secret_core::node::db::generic_db::{FindOneQuery, SaveCommand, UserPasswordEntity, UserPasswordsRepo};

    use crate::db::{WasmDbError, DB_NAME};
    use crate::{idbGet, idbSave};

    pub mod store_conf {
        pub const STORE_NAME: &str = "meta_passwords";
    }

    pub struct UserPasswordsWasmRepo {}

    #[async_trait(? Send)]
    impl SaveCommand<UserPasswordEntity, WasmDbError> for UserPasswordsWasmRepo {

        async fn save(&self, key: &str, pass: &UserPasswordEntity) -> Result<(), Self::Error> {
            let pass_js = serde_wasm_bindgen::to_value(pass)?;
            idbSave(DB_NAME, store_conf::STORE_NAME, key, pass_js).await;
            Ok(())
        }
    }

    #[async_trait(? Send)]
    impl FindOneQuery<UserPasswordEntity, WasmDbError> for UserPasswordsWasmRepo {

        async fn find_one(&self, key: &str) -> Result<Option<UserPasswordEntity>, Self::Error> {
            let pass_js = idbGet(DB_NAME, store_conf::STORE_NAME, key).await;
            if pass_js.is_undefined() {
                Ok(None)
            } else {
                let pass = serde_wasm_bindgen::from_value(pass_js)?;
                Ok(Some(pass))
            }
        }
    }
}
