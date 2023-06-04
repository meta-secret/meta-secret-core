use crate::node::db::commit_log;
use crate::node::db::meta_db::CommitLogStore;
use crate::node::db::models::{KeyIdGen, KvLogEvent, LogCommandError, MetaDb};
use std::rc::Rc;

async fn sync<R: CommitLogStore>(repo: R, meta_db: MetaDb) -> Result<MetaDb, LogCommandError> {
    let mut log_events: Vec<KvLogEvent> = vec![];

    let mut tail_id = meta_db.vault_store.tail_id.clone().unwrap();

    loop {
        // update MetaDb with commit log events
        let log_event_result = repo.find_one(tail_id.key_id.as_str()).await;
        match log_event_result {
            Ok(maybe_log_event) => {
                match maybe_log_event {
                    None => {
                        //no new records in the database
                        break;
                    }
                    Some(log_event) => {
                        log_events.push(log_event);
                        tail_id = tail_id.next();
                    }
                }
            }
            Err(_) => {
                panic!("Db error");
            }
        }
    }

    //apply log events to meta_db
    commit_log::apply(Rc::new(log_events), meta_db)
}
