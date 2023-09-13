use std::sync::Arc;

use anyhow::anyhow;
use flume::{Receiver, Sender};
use crate::node::common::task_runner::TaskRunner;

use crate::node::db::events::common::VaultInfo;
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::generic_db::KvLogEventRepo;
use crate::node::db::meta_db::meta_db_view::{MetaDb, TailId};
use crate::node::db::meta_db::store::meta_pass_store::MetaPassStore;
use crate::node::db::meta_db::store::vault_store::VaultStore;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::logger::MetaLogger;

pub struct MetaDbService<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub persistent_obj: Arc<PersistentObject<Repo, Logger>>,
    pub repo: Arc<Repo>,
    pub logger: Arc<Logger>,
    pub meta_db_id: String,

    service_sender: Sender<MetaDbRequestMessage>,
    service_receiver: Receiver<MetaDbRequestMessage>,

    client_sender: Sender<MetaDbResponseMessage>,
    client_receiver: Receiver<MetaDbResponseMessage>,
}

pub struct MetaDbServiceTaskRunner<Runner: TaskRunner, Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub meta_db_service: Arc<MetaDbService<Repo, Logger>>,
    pub task_runner: Arc<Runner>
}

impl<Runner: TaskRunner, Repo: KvLogEventRepo, Logger: MetaLogger> MetaDbServiceTaskRunner<Runner, Repo, Logger> {
    pub async fn run_task(&self) {
        let service = self.meta_db_service.clone();
        self.task_runner.spawn(async move {
            service.run().await;
        }).await;
    }
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> MetaDbService<Repo, Logger> {
    pub fn new(db_id: String, persistent_object: Arc<PersistentObject<Repo, Logger>>) -> Self {
        let (service_sender, service_receiver) = flume::bounded(1);
        let (client_sender, client_receiver) = flume::bounded(1);

        MetaDbService {
            persistent_obj: persistent_object.clone(),
            repo: persistent_object.repo.clone(),
            logger: persistent_object.logger.clone(),
            meta_db_id: db_id,

            service_sender,
            service_receiver,

            client_sender,
            client_receiver,
        }
    }
}

enum MetaDbRequestMessage {
    Sync,
    SetVault { vault_name: String },
    GetVaultInfo { vault_id: ObjectId },
    GetVaultStore,
    GetMetaPassStore,
}

enum MetaDbResponseMessage {
    VaultInfo { vault: VaultInfo },
    VaultStore { vault_store: VaultStore },
    MetaPassStore { meta_pass_store: MetaPassStore },
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> MetaDbService<Repo, Logger> {
    pub async fn sync_db(&self) {
        let _ = self.service_sender
            .send_async(MetaDbRequestMessage::Sync)
            .await;
    }

    pub async fn update_with_vault(&self, vault_name: String) {
        let _ = self.service_sender
            .send_async(MetaDbRequestMessage::SetVault { vault_name })
            .await;
        self.sync_db().await;
    }

    pub async fn get_vault_info(&self, vault_id: ObjectId) -> anyhow::Result<VaultInfo> {
        let _ = self.service_sender
            .send_async(MetaDbRequestMessage::GetVaultInfo { vault_id })
            .await;

        let msg = self.client_receiver.recv_async().await?;

        match msg {
            MetaDbResponseMessage::VaultInfo { vault } => {
                Ok(vault)
            }
            _ => {
                Err(anyhow!("Invalid message"))
            }
        }
    }

    pub async fn get_vault_store(&self) -> anyhow::Result<VaultStore> {
        let _ = self.service_sender
            .send_async(MetaDbRequestMessage::GetVaultStore)
            .await;

        let msg = self.client_receiver.recv_async().await?;

        match msg {
            MetaDbResponseMessage::VaultStore { vault_store } => {
                Ok(vault_store)
            }
            _ => Err(anyhow!("Invalid message"))
        }
    }

    pub async fn get_meta_pass_store(&self) -> anyhow::Result<MetaPassStore> {
        let _ = self.service_sender
            .send_async(MetaDbRequestMessage::GetMetaPassStore)
            .await;

        let msg = self.client_receiver.recv_async().await?;

        match msg {
            MetaDbResponseMessage::MetaPassStore { meta_pass_store } => {
                Ok(meta_pass_store)
            }
            _ => Err(anyhow!("Invalid message"))
        }
    }

    pub async fn run(&self) {
        let mut meta_db = MetaDb::new(self.meta_db_id.clone(), self.logger.clone());

        while let Ok(msg) = self.service_receiver.recv_async().await {
            match msg {
                MetaDbRequestMessage::Sync => {
                    self.sync_meta_db(&mut meta_db).await
                }
                MetaDbRequestMessage::SetVault { vault_name } => {
                    meta_db.update_vault_info(vault_name.as_str())
                }
                MetaDbRequestMessage::GetVaultInfo { vault_id } => {
                    let vault_info = if meta_db.global_index_store.contains(vault_id.unit_id().id_str()) {
                        //if the vault is already present:
                        match &meta_db.vault_store {
                            VaultStore::Store { vault, .. } => {
                                VaultInfo::Member { vault: vault.clone() }
                            }
                            _ => VaultInfo::NotMember
                        }
                    } else {
                        VaultInfo::NotFound
                    };

                    let response = MetaDbResponseMessage::VaultInfo { vault: vault_info };
                    let _ = self.client_sender.send_async(response).await;
                }
                MetaDbRequestMessage::GetVaultStore => {
                    let response = MetaDbResponseMessage::VaultStore {
                        vault_store: meta_db.vault_store.clone()
                    };
                    let _ = self.client_sender.send_async(response).await;
                }
                MetaDbRequestMessage::GetMetaPassStore => {
                    let response = MetaDbResponseMessage::MetaPassStore {
                        meta_pass_store: meta_db.meta_pass_store.clone()
                    };
                    let _ = self.client_sender.send_async(response).await;
                }
            }
        }
    }

    async fn sync_meta_db(&self, meta_db: &mut MetaDb<Logger>) {
        self.logger.debug("Sync meta db");

        let vault_events = match meta_db.vault_store.tail_id() {
            None => {
                vec![]
            }
            Some(tail_id) => self.persistent_obj.find_object_events(&tail_id).await,
        };

        //sync global index
        let gi_events = {
            let maybe_gi_tail_id = meta_db.global_index_store.tail_id();

            match maybe_gi_tail_id {
                None => {
                    self.persistent_obj
                        .find_object_events(&ObjectId::global_index_unit())
                        .await
                }
                Some(tail_id) => self
                    .persistent_obj
                    .find_object_events(&tail_id).await,
            }
        };

        let meta_pass_events = {
            match meta_db.meta_pass_store.tail_id() {
                None => {
                    vec![]
                }
                Some(tail_id) => self
                    .persistent_obj
                    .find_object_events(&tail_id).await,
            }
        };

        let mut commit_log = vec![];
        commit_log.extend(vault_events);
        commit_log.extend(gi_events);
        commit_log.extend(meta_pass_events);

        meta_db.apply(commit_log);
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::meta_db::meta_db_service::MetaDbService;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::logger::{DefaultMetaLogger, LoggerId};

    #[tokio::test]
    async fn test() {
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let logger = Arc::new(DefaultMetaLogger { id: LoggerId::Client });
        let persistent_object = Arc::new(PersistentObject::new(repo, logger));
        let _manager = MetaDbService::new(String::from("test_db"), persistent_object);
    }
}
