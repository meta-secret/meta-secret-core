use std::sync::Arc;

use anyhow::anyhow;
use tracing_attributes::instrument;

use crate::node::common::model::user::{UserData, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::local::CredentialsObject;
use crate::node::db::events::vault_event::VaultStatusObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn find_for_default_user(&self) -> anyhow::Result<VaultStatus> {
        let creds_repo = CredentialsRepo {
            repo: self.p_obj.repo.clone()
        };

        let creds = creds_repo.get().await?;

        match creds {
            CredentialsObject::Device { .. } => {
                Err(anyhow!("User not found found"))
            }
            CredentialsObject::DefaultUser { event } => {
                self.find(event.value.user()).await
            }
        }
    }

    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> anyhow::Result<VaultStatus> {
        let membership = self.vault_status(user).await?;

        match membership {
            UserMembership::Outsider(outsider) => {
                Ok(VaultStatus::Outsider(outsider))
            }
            UserMembership::Member(member) => {
                let maybe_vault = {
                    let vault_desc = VaultDescriptor::vault(member.user_data.vault_name.clone());
                    self.p_obj.find_tail_event(vault_desc).await?
                };

                if let Some(vault_event) = maybe_vault {
                    VaultStatus::try_from(vault_event, member.user_data)?
                } else {
                    Ok(VaultStatus::Outsider(UserDataOutsider::unknown(member.user_data)))
                }
            }
        }
    }

    pub async fn vault_status(&self, user_data: UserData) -> anyhow::Result<VaultStatus> {
        let desc = VaultDescriptor::vault_status(user_data.user_id());
        let maybe_tail_event = self.p_obj.find_tail_event(desc).await?;

        match maybe_tail_event {
            None => {
                Ok(VaultStatus::unknown(user_data))
            }
            Some(tail_event) => {
                let vault_status_obj = VaultStatusObject::try_from(tail_event)?;

                match vault_status_obj {
                    VaultStatusObject::Unit { .. } => {
                        UserMembership::Outsider(UserDataOutsider::unknown(user_data))
                    }
                    VaultStatusObject::Genesis { .. } => {
                        UserMembership::Outsider(UserDataOutsider::unknown(user_data))
                    }
                    VaultStatusObject::Status { event } => {
                        event.value
                    }
                }
            }
        }
    }
}