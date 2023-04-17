use crate::node::db::commit_log::KvLogEvent;
use crate::node::db::db::{GetCommand, SaveCommand};

pub trait CommitLogRepo: SaveCommand<KvLogEvent> + GetCommand<KvLogEvent> {}
