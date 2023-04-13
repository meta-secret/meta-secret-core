use crate::node::commit_log::KvLogEvent;
use crate::node::db::{GetCommand, SaveCommand};

pub trait CommitLogRepo: SaveCommand<KvLogEvent> + GetCommand<KvLogEvent> {}
