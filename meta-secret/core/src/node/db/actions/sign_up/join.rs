use crate::node::common::model::user::common::{UserData, UserDataOutsiderStatus, UserMembership};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::bail;
use anyhow::Result;
use std::sync::Arc;

pub struct AcceptJoinAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub member: VaultMember,
}

impl<Repo: KvLogEventRepo> AcceptJoinAction<Repo> {
    
    pub async fn accept(&self, candidate: UserData) -> Result<()> {
        let candidate_membership = self.member.vault.membership(candidate);

        match candidate_membership {
            UserMembership::Outsider(outsider) => {
                match outsider.status {
                    UserDataOutsiderStatus::NonMember => {
                        let p_device_log = PersistentDeviceLog {
                            p_obj: self.p_obj.clone(),
                        };

                        p_device_log
                            .save_accept_join_request_event(self.member.member.clone(), outsider)
                            .await
                    }
                    UserDataOutsiderStatus::Pending => {
                        bail!("User already in pending state")
                    }
                    UserDataOutsiderStatus::Declined => {
                        bail!("User request already declined")
                    }
                }
            }
            UserMembership::Member(_) => {
                bail!("Membership cannot be accepted. Invalid state")
            }
        }
    }
}