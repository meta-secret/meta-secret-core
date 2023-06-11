







/*use meta_secret_core::node::app::meta_app::PersistentMetaVault;

#[async_trait(? Send)]
trait InMemMetaVaultRepo {

}

#[async_trait(? Send)]
struct InMemMetaVaultService {
    pub xxx: String
}

#[async_trait(? Send)]
impl InMemMetaVaultRepo for InMemMetaVaultService {

}

impl InMemMetaVaultService {
    fn yay(&mut self) {
        self.xxx = String::from("qwe");
    }
}

#[cfg(test)]
mod test {
    use std::rc::Rc;
    use super::*;

    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::models::DeviceInfo;
    use meta_secret_core::node::app::meta_app::MetaVaultService;
    use meta_secret_core::node::db::commit_log;
    use meta_secret_core::node::db::events::global_index;
    use meta_secret_core::node::db::events::join::join_cluster_request;
    use meta_secret_core::node::db::events::sign_up::sign_up_request;
    use meta_secret_core::node::db::models::{KeyIdGen, KvKeyId, ObjectType, VaultId};
    use meta_secret_core::node::server::meta_server::{MetaServer, MetaServerNode, SyncRequest, VaultSyncRequest};
    use meta_server_emulator::server::sqlite_migration::EmbeddedMigrationsTool;
    use meta_server_emulator::server::sqlite_store::SqlIteStore;

    #[tokio::test]
    async fn test() {
        let mut service = InMemMetaVaultService {
            xxx: "".to_string(),
        };

        service.create_meta_vault();
        service.yay();
        service.save()
    }
}*/