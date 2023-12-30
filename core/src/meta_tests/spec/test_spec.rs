use anyhow::Result;
use async_trait::async_trait;

#[async_trait(? Send)]
pub trait TestSpec {
    async fn check(&self) -> Result<()>;
}