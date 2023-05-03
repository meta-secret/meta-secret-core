use crate::node::db::db::{FindOneQuery, FindQuery, SaveCommand};
use crate::node::db::models::KvLogEvent;

pub trait CommitLogRepo: SaveCommand<KvLogEvent> + FindQuery<KvLogEvent> + FindOneQuery<KvLogEvent> {}
