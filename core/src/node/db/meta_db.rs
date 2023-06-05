use crate::node::db::db::{FindOneQuery, FindQuery, SaveCommand};
use crate::node::db::models::KvLogEvent;

pub trait CommitLogStore: SaveCommand<KvLogEvent>  + FindOneQuery<KvLogEvent> {}

pub trait VaultsIndexRepo: SaveCommand<KvLogEvent> + FindQuery<KvLogEvent> + FindOneQuery<KvLogEvent> {}
