use async_trait::async_trait;

use crate::models::{AeadAuthData, MetaPasswordDoc, MetaPasswordId, MetaVault, UserCredentials};
use crate::shared_secret::shared_secret::UserShareDto;

#[async_trait(? Send)]
pub trait GenericRepo<T> {
    type Error: std::error::Error;

    async fn save(&self, key: &str, value: &T) -> Result<(), Self::Error>;
    async fn get(&self, key: &str) -> Result<Option<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    type Error: std::error::Error;

    async fn find(&self, key: &str) -> Result<Vec<T>, Self::Error>;
}

pub trait UserCredentialsRepo: GenericRepo<UserCredentials> {}

pub trait MetaVaultRepo: GenericRepo<MetaVault> {}

pub trait UserPasswordsRepo: GenericRepo<UserPasswordEntity> {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPasswordEntity {
    pub meta_pass_id: MetaPasswordId,
    /// Encrypted UserShareDto-s
    pub shares: Vec<AeadAuthData>,
}
