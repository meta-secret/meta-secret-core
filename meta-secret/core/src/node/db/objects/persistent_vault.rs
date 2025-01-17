use std::sync::Arc;

use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider};
use crate::node::common::model::vault::vault::{VaultMember, VaultName, VaultStatus};
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, ObjectId};
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault_event::{VaultActionEvent, VaultObject};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{anyhow, bail, Result};
use tracing_attributes::instrument;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTail {
    pub vault_log: ObjectId,
    pub vault: ObjectId,
    pub vault_status: ObjectId,
}

pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn vault_log(&self, vault_name: VaultName) -> Result<Option<VaultLogObject>> {
        //vault actions
        let vault_log_desc = VaultDescriptor::VaultLog(vault_name).to_obj_desc();
        let maybe_vault_log_event = self.p_obj
            .find_tail_event::<VaultLogObject>(vault_log_desc)
            .await?;

        Ok(maybe_vault_log_event)
    }
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn vault_tail(&self, user: UserData) -> Result<VaultTail> {
        let vault_log_free_id = {
            let obj_desc = VaultDescriptor::vault_log(user.vault_name());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        let vault_free_id = {
            let obj_desc = VaultDescriptor::vault(user.vault_name());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        let vault_status_free_id = {
            let obj_desc = VaultDescriptor::vault_membership(user.user_id());
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
    pub async fn get_vault(&self, user_data: &UserData) -> anyhow::Result<(ArtifactId, VaultData)> {
        let p_vault = PersistentVault {
            p_obj: self.p_obj.clone(),
        };

        let vault_status = p_vault.find(user_data.clone()).await?;
        match vault_status {
            VaultStatus::NotExists(_) => Err(anyhow!("Vault not found")),
            VaultStatus::Outsider(_) => Err(anyhow!("Sender is not a member of the vault")),
            VaultStatus::Member { member, .. } => {
                //save new vault state
                let vault_desc = VaultDescriptor::vault(member.vault.vault_name.clone());

                let vault_free_id = self
                    .p_obj
                    .find_free_id_by_obj_desc(vault_desc.clone())
                    .await?;

                let ObjectId::Artifact(vault_artifact_id) = vault_free_id else {
                    return Err(anyhow!(
                        "Invalid vault id, must be ArtifactId, but it's: {:?}",
                        vault_free_id
                    ));
                };

                anyhow::Ok((vault_artifact_id, member.vault))
            }
        }
    }

    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> Result<VaultStatus> {
        let maybe_vault_event = {
            let vault_desc = VaultDescriptor::vault(user.vault_name());
            self.p_obj.find_tail_event::<VaultObject>(vault_desc).await?
        };

        let gi_and_vault = maybe_vault_event;
        match gi_and_vault {
            None => Ok(VaultStatus::NotExists(user)),
            //There is no vault table on local machine, but it is present in global index,
            //which means, current user is outsider
            Some(vault_obj) => {
                match vault_obj {
                    VaultObject::Unit(_) => {
                        bail!("Invalid state. Vault has only unit event")
                    }
                    VaultObject::Genesis(_) => {
                        bail!("Invalid state. Genesis event is not enough")
                    }
                    VaultObject::Vault(KvLogEvent {
                        value: vault_data, ..
                    }) => {
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
        }
    }

    pub async fn save_vault_log_event(&self, action_event: VaultActionEvent) -> anyhow::Result<()> {
        let desc = VaultDescriptor::vault_log(action_event.vault_name());
        let free_id = self.p_obj.find_free_id_by_obj_desc(desc.clone()).await?;

        let ObjectId::Artifact(artifact_id) = free_id else {
            bail!("Invalid vault log state, expected artifact id");
        };

        let event = VaultLogObject::Action(KvLogEvent {
            key: KvKey::artifact(desc, artifact_id),
            value: action_event.clone(),
        });

        let vault_log_event = GenericKvLogEvent::VaultLog(event);

        self.p_obj.repo.save(vault_log_event).await?;

        Ok(())
    }

    async fn vault_log_events(&self, vault_name: VaultName) -> Result<Vec<VaultLogObject>> {
        let desc = VaultDescriptor::vault_log(vault_name);
        let events = self.p_obj.find_object_events(ObjectId::unit(desc)).await?;

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
    use crate::node::common::model::user::common::UserData;
    use crate::node::db::events::vault::vault_log_event::VaultLogObject;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use std::sync::Arc;
    use anyhow::{bail, Result};
    use crate::node::common::model::device::common::DeviceName;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
    use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
    use crate::node::db::events::object_id::{VaultGenesisEvent, VaultUnitEvent};

    pub struct VaultLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> VaultLogSpec<Repo> {
        pub async fn verify_initial_state(&self) -> Result<()> {
            let events = self.vault_log().await?;
            assert_eq!(2, events.len());

            if let VaultLogObject::Unit(VaultUnitEvent(unit_kv)) = events.first().unwrap() {
                if let ObjectDescriptor::Vault(VaultDescriptor::VaultLog(name)) = &unit_kv.key.obj_desc {
                    assert_eq!(name, &VaultName::test());
                } else {
                    bail!("Expected unit to be a vault");
                }

                assert_eq!(unit_kv.value, VaultName::test());
            } else {
                bail!("Invalid unit event");
            }

            if let VaultLogObject::Genesis(VaultGenesisEvent(event)) = events.get(1).unwrap() {
                assert_eq!(event.value.device.device_name, DeviceName::client());
            } else {
                bail!("Invalid genesis event");
            }
            
            Ok(())
        }

        async fn vault_log(&self) -> Result<Vec<VaultLogObject>> {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };
            p_vault.vault_log_events(self.user.vault_name()).await
        }
    }
}
