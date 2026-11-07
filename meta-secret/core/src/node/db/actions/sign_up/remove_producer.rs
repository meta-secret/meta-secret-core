//! The same existing request/policy builder is shared by checked and legacy ports.
use super::RemoveAction;
use crate::node::{
    common::model::user::common::UserDataMember,
    db::{
        events::vault::vault_log_event::{RemoveMemberRequestEvent, RemoveMemberShareTransfer},
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::Result;

impl<R: KvLogEventRepo> RemoveAction<R> {
    pub(crate) fn build_request(
        &self,
        target: UserDataMember,
        share_transfers: Vec<RemoveMemberShareTransfer>,
    ) -> Result<RemoveMemberRequestEvent> {
        let request = RemoveMemberRequestEvent {
            sender: self.member.member.clone(),
            target,
            share_transfers,
        };
        self.member.vault.ensure_can_remove_member(&request, None)?;
        self.member
            .vault
            .ensure_valid_removal_transfers(&request, &request.share_transfers)?;
        Ok(request)
    }
}
