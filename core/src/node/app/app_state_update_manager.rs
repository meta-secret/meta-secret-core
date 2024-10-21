use crate::node::db::repo::generic_db::KvLogEventRepo;
use std::sync::Arc;

pub struct ApplicationManagerConfigurator<Repo>
where
    Repo: KvLogEventRepo,
{
    pub client_repo: Arc<Repo>,
    pub server_repo: Arc<Repo>,
    pub device_repo: Arc<Repo>,
}
