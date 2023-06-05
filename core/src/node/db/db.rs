use async_trait::async_trait;

use crate::models::{MetaPasswordId, MetaVault, SecretDistributionDocData, UserCredentials};

#[async_trait(? Send)]
pub trait SaveCommand<Data> {
    type Error: std::error::Error;

    async fn save(&self, key: &str, value: &Data) -> Result<(), Self::Error>;
}

#[async_trait(? Send)]
pub trait FindOneQuery<Data> {
    type Error: std::error::Error;

    async fn find_one(&self, key: &str) -> Result<Option<Data>, Self::Error>;
}


#[async_trait(? Send)]
pub trait FindQuery<T> {
    type Error: std::error::Error;

    async fn find(&self, key: &str) -> Result<Vec<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindByAttrQuery<T> {
    type Error: std::error::Error;

    async fn find_by(&self, attr_name: &str) -> Result<Vec<T>, Self::Error>;
}

#[async_trait(? Send)]
pub trait FindAllQuery<T> {
    type Error: std::error::Error;

    async fn find_all(&self) -> Result<Vec<T>, Self::Error>;
}

pub trait UserCredentialsRepo: SaveCommand<UserCredentials> + FindQuery<UserCredentials> {}

pub trait MetaVaultRepo: SaveCommand<MetaVault> + FindQuery<MetaVault> {}

pub trait UserPasswordsRepo: SaveCommand<UserPasswordEntity> + FindQuery<UserPasswordEntity> {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPasswordEntity {
    pub meta_pass_id: MetaPasswordId,
    /// Encrypted UserShareDto-s
    pub shares: Vec<SecretDistributionDocData>,
}
