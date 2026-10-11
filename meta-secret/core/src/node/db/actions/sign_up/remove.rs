use crate::node::common::model::user::common::{
    UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::events::vault::vault_log_event::{
    RemoveMemberDecision, RemoveMemberEvent, RemoveMemberRequestEvent, RemoveMemberShareTransfer,
};
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{Result, bail};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveActionUpdate {
    Accept,
    Decline,
}

pub struct RemoveAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub member: VaultMember,
}

impl<Repo: KvLogEventRepo> RemoveAction<Repo> {
    pub async fn request(
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
        PersistentDeviceLog::from(self.p_obj.clone())
            .save_remove_member_request(request.clone())
            .await?;
        Ok(request)
    }

    pub async fn update(
        &self,
        request: RemoveMemberRequestEvent,
        upd: RemoveActionUpdate,
        share_transfers: Vec<RemoveMemberShareTransfer>,
    ) -> Result<RemoveMemberEvent> {
        let approver = self.member.member.clone();
        self.member
            .vault
            .ensure_can_remove_member(&request, Some(&approver))?;
        self.member
            .vault
            .ensure_valid_removal_transfers(&request, &request.share_transfers)?;
        self.member
            .vault
            .ensure_valid_removal_transfers(&request, &share_transfers)?;
        let (decision, new_status) = match upd {
            RemoveActionUpdate::Accept => (
                RemoveMemberDecision::Accepted,
                UserMembership::Outsider(UserDataOutsider {
                    user_data: request.target.user_data.clone(),
                    status: UserDataOutsiderStatus::Declined,
                }),
            ),
            RemoveActionUpdate::Decline => {
                if !share_transfers.is_empty() {
                    bail!("Declined removal must not contain Key Share transfers")
                }
                (
                    RemoveMemberDecision::Declined,
                    UserMembership::Member(request.target.clone()),
                )
            }
        };
        let update = RemoveMemberEvent {
            request: request.clone(),
            sender: approver,
            target: request.target.clone(),
            new_status,
            decision,
            share_transfers,
        };
        PersistentDeviceLog::from(self.p_obj.clone())
            .save_remove_member_update(update.clone())
            .await?;
        Ok(update)
    }
}
