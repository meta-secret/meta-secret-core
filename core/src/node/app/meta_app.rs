use std::collections::HashMap;
use std::fmt::Error;
use crate::node::db::generic_db::{FindOneQuery, SaveCommand};
use crate::models::device_info::DeviceInfo;
use crate::models::meta_vault::MetaVault;
use async_trait::async_trait;
use crate::crypto::keys::KeyManager;
use crate::node::db::models::KvLogEvent;
use crate::node::server::meta_server::{DataSync, DataTransport, MetaServerContextState, MetaServer};

//запилить клиентскую часть!
//нам надо регистрацию мета волта взять из веб cli
//потом юзер креденшиалы
//потом синхронизацию настроить с серваком
//и наконец - функциональность приложения























/// -----------------------------------------------------------------------------  server side

mod store_conf {
    //pub const STORE_NAME: &str = "meta_vault";
    pub const KEY_NAME: &str = "vault";
}

use crate::node::db::generic_db::KvLogEventRepo;


impl KvLogEventRepo for MetaServerContextState {}

#[async_trait(? Send)]
impl FindOneQuery<KvLogEvent> for MetaServerContextState {
    type Error = Error;

    async fn find_one(&self, key: &str) -> Result<Option<KvLogEvent>, Self::Error> {
        Ok(None)
    }
}

#[async_trait(? Send)]
impl SaveCommand<KvLogEvent> for MetaServerContextState {
    type Error = Error;

    async fn save(&self, key: &str, value: &KvLogEvent) -> Result<(), Self::Error> {
        todo!()
    }
}

impl MetaServer for MetaServerContextState {

}

fn yay() {
    let node = MetaServerContextState {
        km: KeyManager::generate(),
        global_index_tail_id: None,
    };

    //let xxx = node.accept_sign_up_request().await;
}
