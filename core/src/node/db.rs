use std::fmt::Error;

pub trait BasicRepo<T> {
    fn save(self, entity: &T);
    fn get(self) -> Result<T, Error>;
}