use std::sync::Arc;

use anyhow::bail;
use tracing_attributes::instrument;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::vault_event::VaultMembershipObject;
use crate::node::db::objects::global_index::ClientPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> anyhow::Result<VaultStatus> {
        let membership = self.vault_membership(user.clone()).await?;

        let p_gi = ClientPersistentGlobalIndex {
            p_obj: self.p_obj.clone(),
        };
        let vault_exists = p_gi.exists(user.vault_name()).await?;
        if !vault_exists {
            return Ok(VaultStatus::NotExists(user));
        }

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

    async fn vault_membership(&self, user_data: UserData) -> anyhow::Result<UserMembership> {
        let desc = VaultDescriptor::vault_membership(user_data.user_id());
        let maybe_tail_event = self.p_obj.find_tail_event(desc).await?;

        match maybe_tail_event {
            None => Ok(UserMembership::Outsider(UserDataOutsider::non_member(
                user_data.clone(),
            ))),
            Some(tail_event) => {
                let vault_membership_obj = VaultMembershipObject::try_from(tail_event)?;

                match vault_membership_obj {
                    VaultMembershipObject::Unit { .. } => Ok(UserMembership::Outsider(UserDataOutsider::non_member(
                        user_data.clone(),
                    ))),
                    VaultMembershipObject::Genesis { .. } => Ok(UserMembership::Outsider(
                        UserDataOutsider::non_member(user_data.clone()),
                    )),
                    VaultMembershipObject::Membership(event) => Ok(event.value),
                }
            }
        }
    }
}
