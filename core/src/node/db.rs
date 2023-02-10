use async_trait::async_trait;

use crate::models::{MetaVault, UserCredentials};

#[async_trait(? Send)]
pub trait GenericRepo<T> {
    type Error: std::error::Error;

    async fn save(&self, key: &str, value: &T) -> Result<(), Self::Error>;
    async fn get(&self, key: &str) -> Result<Option<T>, Self::Error>;
}

pub trait UserCredentialsRepo: GenericRepo<UserCredentials> {}

pub trait MetaVaultRepo: GenericRepo<MetaVault> {}
