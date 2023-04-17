use async_trait::async_trait;

use crate::models::{MetaPasswordId, MetaVault, SecretDistributionDocData, UserCredentials};

#[async_trait(? Send)]
pub trait SaveCommand<T> {
    type Error: std::error::Error;
    async fn save(&self, key: &str, value: &T) -> Result<(), Self::Error>;
}

#[async_trait(? Send)]
pub trait GetCommand<T> {
    type Error: std::error::Error;
    async fn get(&self, key: &str) -> Result<Option<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindQuery<T> {
    type Error: std::error::Error;

    async fn find(&self, key: &str) -> Result<Vec<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindAllQuery<T> {
    type Error: std::error::Error;

    async fn find_all(&self) -> Result<Vec<T>, Self::Error>;
}

pub trait UserCredentialsRepo: SaveCommand<UserCredentials> + GetCommand<UserCredentials> {}

pub trait MetaVaultRepo: SaveCommand<MetaVault> + GetCommand<MetaVault> {}

pub trait UserPasswordsRepo: SaveCommand<UserPasswordEntity> + GetCommand<UserPasswordEntity> {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPasswordEntity {
    pub meta_pass_id: MetaPasswordId,
    /// Encrypted UserShareDto-s
    pub shares: Vec<SecretDistributionDocData>,
}
