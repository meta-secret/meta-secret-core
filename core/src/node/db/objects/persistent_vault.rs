use std::sync::Arc;

use anyhow::{anyhow, bail};
use tracing_attributes::instrument;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::{VaultData, VaultName, VaultStatus};
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, ObjectId};
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault_event::VaultMembershipObject;
use crate::node::db::objects::global_index::ClientPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {

    pub async fn get_vault(&self, user_data: &UserData) -> anyhow::Result<(ArtifactId, VaultData)> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(user_data.clone()).await?;
        match vault_status {
            VaultStatus::NotExists(_) => {
                Err(anyhow!("Vault not found"))
            }
            VaultStatus::Outsider(_) => {
                Err(anyhow!("Sender is not a member of the vault"))
            }
            VaultStatus::Member { vault, .. } => {
                //save new vault state
                let vault_desc = VaultDescriptor::vault(vault.vault_name.clone());

                let vault_free_id = self.p_obj
                    .find_free_id_by_obj_desc(vault_desc.clone())
                    .await?;

                let ObjectId::Artifact(vault_artifact_id) = vault_free_id else {
                    return Err(anyhow!("Invalid vault id, must be ArtifactId, but it's: {:?}",vault_free_id));
                };

                anyhow::Ok((vault_artifact_id, vault))
            }
        }
    }
    
    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> anyhow::Result<VaultStatus> {
        let p_gi = ClientPersistentGlobalIndex { p_obj: self.p_obj.clone() };
        let vault_exists = p_gi.exists(user.vault_name()).await?;
        if !vault_exists {
            return Ok(VaultStatus::NotExists(user));
        }

        let vault_desc = VaultDescriptor::vault(user.vault_name());
        let maybe_vault_event = self.p_obj
            .find_tail_event(vault_desc)
            .await?;
        match maybe_vault_event {
            None => {
                Ok(VaultStatus::NotExists(user))
            }
            Some(vault_event) => {
                let vault_obj = vault_event.vault()?;
                Ok(vault_obj.status(user.clone()))
            }
        }
    }

    async fn vault_log_events(&self, vault_name: VaultName) -> anyhow::Result<Vec<VaultLogObject>> {
        let desc = VaultDescriptor::vault_log(vault_name);
        let events = self.p_obj
            .find_object_events(ObjectId::unit(desc))
            .await?;

        let mut vault_log_events = vec![];
        for event in events {
            let vault_log_event = event.vault_log()?;
            vault_log_events.push(vault_log_event);
        }

        Ok(vault_log_events)
    }
}


#[cfg(test)]
pub mod spec {
    use std::sync::Arc;
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::events::vault::vault_log_event::VaultLogObject;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::generic_db::KvLogEventRepo;

    pub struct VaultLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData
    }

    impl<Repo: KvLogEventRepo> VaultLogSpec<Repo> {
        pub async fn verify_initial_state(&self) -> anyhow::Result<()> {
            let events = self.vault_log().await?;
            assert_eq!(3, events.len());

            Ok(())
        }

        async fn vault_log(&self) -> anyhow::Result<Vec<VaultLogObject>> {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };
            p_vault.vault_log_events(self.user.vault_name()).await
        }
    }
}