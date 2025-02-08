use std::sync::Arc;

use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider};
use crate::node::common::model::vault::vault::{VaultMember, VaultName, VaultStatus};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::descriptors::vault_descriptor::{
    VaultDescriptor, VaultLogDescriptor, VaultMembershipDescriptor,
};
use crate::node::db::events::kv_log_event::KvLogEvent;
use crate::node::db::events::object_id::{ArtifactId, Next};
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::{
    VaultActionEvents, VaultActionRequestEvent, VaultLogObject,
};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{anyhow, bail, Result};
use tracing_attributes::instrument;
use crate::node::db::events::generic_log_event::KeyExtractor;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTail {
    pub vault_log: ArtifactId,
    pub vault: ArtifactId,
    pub vault_status: ArtifactId,
}

pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn vault_log(&self, vault_name: VaultName) -> Result<Option<VaultLogObject>> {
        //vault actions
        let vault_log_desc = VaultLogDescriptor::from(vault_name);
        let maybe_vault_log_event = self.p_obj.find_tail_event(vault_log_desc).await?;

        Ok(maybe_vault_log_event)
    }
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn vault_tail(&self, user: UserData) -> Result<VaultTail> {
        let vault_log_free_id = {
            let obj_desc = VaultLogDescriptor::from(user.vault_name());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        let vault_free_id = {
            let obj_desc = VaultDescriptor::from(user.vault_name());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        let vault_status_free_id = {
            let obj_desc = VaultMembershipDescriptor::from(user.user_id());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        Ok(VaultTail {
            vault_log: vault_log_free_id,
            vault: vault_free_id,
            vault_status: vault_status_free_id,
        })
    }
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn get_vault(&self, user_data: &UserData) -> Result<(ArtifactId, VaultData)> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(user_data.clone()).await?;
        match vault_status {
            VaultStatus::NotExists(_) => {
                bail!("Vault not found")
            }
            VaultStatus::Outsider(_) => {
                bail!("Sender is not a member of the vault")
            }
            VaultStatus::Member { member, .. } => {
                //save new vault state
                let vault_desc = VaultDescriptor::from(member.vault.vault_name.clone());

                let vault_free_id = self.p_obj.find_free_id_by_obj_desc(vault_desc).await?;

                Ok((vault_free_id, member.vault))
            }
        }
    }

    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> Result<VaultStatus> {
        let maybe_vault_event = {
            let vault_desc = VaultDescriptor::from(user.vault_name());
            self.p_obj.find_tail_event(vault_desc).await?
        };
        
        todo!("Figure out if vault already exists");

        let gi_and_vault = maybe_vault_event;
        match gi_and_vault {
            None => Ok(VaultStatus::NotExists(user)),
            //There is no vault table on local machine, but it is present in global index,
            //which means, current user is outsider
            Some(vault_obj) => {
                let vault_data = vault_obj.0.value;
                if vault_data.is_not_member(&user.device.device_id) {
                    Ok(VaultStatus::Outsider(UserDataOutsider::non_member(user)))
                } else {
                    let p_ss = PersistentSharedSecret {
                        p_obj: self.p_obj.clone(),
                    };
                    let ss_claims = p_ss.get_ss_log_obj(user.vault_name()).await?;

                    Ok(VaultStatus::Member {
                        member: VaultMember {
                            member: UserDataMember { user_data: user },
                            vault: vault_data,
                        },
                        ss_claims,
                    })
                }
            }
        }
    }

    pub async fn save_vault_log_events(
        &self,
        events: VaultActionEvents,
        vault_name: VaultName,
    ) -> Result<()> {
        let kv = self.get_vault_log_artifact(vault_name).await?;
        let next_key = kv.key().next();

        let vault_log_event = VaultLogObject(KvLogEvent {
            key: next_key,
            value: events,
        });

        self.p_obj.repo.save(vault_log_event).await?;

        Ok(())
    }

    pub async fn save_vault_log_request_event(
        &self,
        action_event: VaultActionRequestEvent,
    ) -> Result<()> {
        let kv = self
            .get_vault_log_artifact(action_event.vault_name())
            .await?;
        let next_key = kv.key().next();

        let vault_log_event = VaultLogObject(KvLogEvent {
            key: next_key,
            value: kv.0.value.add(action_event),
        });

        self.p_obj.repo.save(vault_log_event).await?;

        Ok(())
    }

    pub async fn get_vault_log_artifact(
        &self,
        vault_name: VaultName,
    ) -> Result<VaultLogObject> {
        let desc = VaultLogDescriptor::from(vault_name);
        let maybe_vault_log_event = self.p_obj.find_tail_event(desc.clone()).await?;

        let Some(vault_log_obj) = maybe_vault_log_event else {
            bail!("Invalid state, vault log is empty");
        };

        Ok(vault_log_obj)
    }

    async fn vault_log_events(&self, vault_name: VaultName) -> Result<Vec<VaultLogObject>> {
        let desc = VaultLogDescriptor::from(vault_name);
        let events = self
            .p_obj
            .find_object_events::<VaultLogObject>(ArtifactId::from(desc))
            .await?;

        Ok(events)
    }
}

#[cfg(test)]
pub mod spec {
    use crate::node::common::model::user::common::UserData;
    use crate::node::common::model::vault::vault::{VaultName, VaultStatus};
    use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectName};
    use crate::node::db::events::vault::vault_log_event::VaultLogObject;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::{bail, Result};
    use std::sync::Arc;

    pub struct VaultLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> VaultLogSpec<Repo> {
        
        async fn vault_log(&self) -> Result<Vec<VaultLogObject>> {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };
            p_vault.vault_log_events(self.user.vault_name()).await
        }
    }

    pub struct VaultSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> VaultSpec<Repo> {
        pub async fn verify_user_is_a_member(&self) -> Result<()> {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };

            let vault_status = p_vault.find(self.user.clone()).await?;

            let VaultStatus::Member { .. } = &vault_status else {
                bail!("Client is not a vault member: {:?}", vault_status);
            };

            Ok(())
        }
    }
}
