use crate::node::common::model::user::common::{
    UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::events::vault::vault_log_event::{
    RemoveMemberEvent, RemoveMemberRequestEvent, RemoveMemberShareTransfer,
};
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{bail, Result};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveActionUpdate {
    Accept,
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
        self.member
            .vault
            .ensure_can_remove_member(&request, None)?;
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
    ) -> Result<()> {
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
        if !matches!(upd, RemoveActionUpdate::Accept) {
            bail!("Unsupported removal action")
        }
        let update = RemoveMemberEvent {
            request: request.clone(),
            sender: approver,
            target: request.target.clone(),
            new_status: UserMembership::Outsider(UserDataOutsider {
                user_data: request.target.user_data,
                status: UserDataOutsiderStatus::Declined,
            }),
            share_transfers,
        };
        PersistentDeviceLog::from(self.p_obj.clone())
            .save_remove_member_update(update)
            .await
    }
}
