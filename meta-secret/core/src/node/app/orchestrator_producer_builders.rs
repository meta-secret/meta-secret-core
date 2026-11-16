//! Reuse existing removal material preparation and local claim canonicalization.
use super::MetaOrchestrator;
use crate::node::{
    common::model::{
        user::common::UserDataMember,
        vault::{vault::VaultMember, vault_data::VaultData},
    },
    db::{
        actions::sign_up::remove::RemoveAction,
        events::vault::vault_log_event::RemoveMemberRequestEvent, repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::Result;

impl<R: KvLogEventRepo> MetaOrchestrator<R> {
    pub(crate) async fn build_remove_member_request(
        &self,
        target: UserDataMember,
    ) -> Result<RemoveMemberRequestEvent> {
        let member = self.get_member().await?;
        let vault = self.get_vault(member.clone()).await?;
        let transfers = if vault.members().len() >= 3 {
            self.build_local_share_transfers_for_removal(&vault, &target)
                .await?
        } else {
            vec![]
        };
        RemoveAction {
            p_obj: self.p_obj.clone(),
            member: VaultMember { member, vault },
        }
        .build_request(target, transfers)
    }

    pub(crate) async fn finalize_local_remove_material(&self, vault: &VaultData) -> Result<()> {
        self.canonicalize_single_device_claims(vault).await
    }
}
