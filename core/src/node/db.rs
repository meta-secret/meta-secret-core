use std::fmt::Error;

pub trait BasicRepo<T> {
    fn save(self, entity: &T);
    async fn get(self) -> Result<T, Error>;
}
