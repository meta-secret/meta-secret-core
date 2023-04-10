use crate::models::KvLogEvent;
use crate::node::db::{GetCommand, SaveCommand};

pub trait CommitLogRepo: SaveCommand<KvLogEvent> + GetCommand<KvLogEvent> {}