use std::fmt::Error;
use async_trait::async_trait;

#[async_trait(? Send)]
pub trait BasicRepo<T> {
    fn save(self, entity: &T);
    async fn get(self) -> Result<T, Error>;
}
