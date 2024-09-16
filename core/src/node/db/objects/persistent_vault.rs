use std::sync::Arc;

use anyhow::bail;
use tracing_attributes::instrument;

use crate::node::common::model::user::{UserData, UserDataMember, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::vault_event::VaultMembershipObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {

    #[instrument(skip_all)]
    pub async fn find_for_default_user(&self) -> anyhow::Result<Option<VaultStatus>> {
        let creds_repo = CredentialsRepo {
            p_obj: self.p_obj.clone(),
        };

        let maybe_creds = creds_repo.get_user_creds().await?;
        
        let Some(user_creds) = maybe_creds else {
            return Ok(None);
        };

        let vault_status = self.find(user_creds.user()).await?;
        Ok(Some(vault_status))
    }

    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> anyhow::Result<VaultStatus> {
        let membership = self.vault_membership(user).await?;

        match membership {
            UserMembership::Outsider(outsider) => Ok(VaultStatus::Outsider(outsider)),
            UserMembership::Member(UserDataMember(member)) => {
                let maybe_vault = {
                    let vault_desc = VaultDescriptor::vault(member.vault_name.clone());
                    self.p_obj.find_tail_event(vault_desc).await?
                };

                if let Some(vault_event) = maybe_vault {
                    let vault_status = vault_event.vault()?.status(member);
                    Ok(vault_status)
                } else {
                    bail!("Invalid db structure. Vault not found");
                }
            }
        }
    }

    pub async fn vault_membership(&self, user_data: UserData) -> anyhow::Result<UserMembership> {
        let desc = VaultDescriptor::vault_membership(user_data.user_id());
        let maybe_tail_event = self.p_obj.find_tail_event(desc).await?;

        match maybe_tail_event {
            None => Ok(UserMembership::Outsider(UserDataOutsider::unknown(user_data))),
            Some(tail_event) => {
                let vault_membership_obj = VaultMembershipObject::try_from(tail_event)?;

                match vault_membership_obj {
                    VaultMembershipObject::Unit { .. } => {
                        Ok(UserMembership::Outsider(UserDataOutsider::unknown(user_data)))
                    }
                    VaultMembershipObject::Genesis { .. } => {
                        Ok(UserMembership::Outsider(UserDataOutsider::unknown(user_data)))
                    }
                    VaultMembershipObject::Membership(event) => Ok(event.value),
                }
            }
        }
    }
}
