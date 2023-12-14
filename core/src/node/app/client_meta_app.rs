use std::sync::Arc;

use tracing::{error, info, instrument, Instrument};

use crate::node::app::meta_app::app_state::{ConfiguredAppState, MemberAppState};
use crate::node::app::sync_gateway::SyncGateway;
use crate::node::common::model::user::UserMembership;
use crate::node::db::descriptors::vault::VaultDescriptor;
use crate::node::db::events::vault_event::VaultStatusObject;
use crate::node::db::objects::device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::secret::MetaDistributor;

pub struct MetaClient<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    sync_gateway: Arc<SyncGateway<Repo>>,
}

impl<Repo: KvLogEventRepo> MetaClient<Repo> {
    #[instrument(skip_all)]
    pub async fn sign_up(&self, curr_state: &ConfiguredAppState) -> MemberAppState {
        self.join_cluster(curr_state.clone()).await;
        let mut updated_app_state = curr_state.app_state.clone();

        // After syncing data we will get the vault status from the server
        self.sync_gateway.sync().await?;

        let maybe_vault_status_event = {
            let desc = VaultDescriptor::vault_status(curr_state.creds.user_id());
            self.persistent_obj.find_tail_event(desc).await?
        };

        let Some(vault_status_event) = maybe_vault_status_event else {
            return MemberAppState {
                app_state: updated_app_state,
                creds: curr_state.creds.clone(),
                vault: None,
                member: None,
            };
        };

        let vault_status = VaultStatusObject::try_from(vault_status_event)?;
        let VaultStatusObject::Status { event: membership } = vault_status else {
            return MemberAppState {
                app_state: updated_app_state,
                creds: curr_state.creds.clone(),
                vault: None,
                member: None,
            };
        };

        let UserMembership::Member(member) = &membership.value else {
            return MemberAppState {
                app_state: updated_app_state,
                creds: curr_state.creds.clone(),
                vault: None,
                member: Some(membership.value.clone()),
            };
        };

        let persistent_vault = PersistentVault {
            p_obj: self.persistent_obj.clone(),
        };

        let maybe_vault = persistent_vault
            .find(member.user_data.clone())
            .await?;

        MemberAppState {
            app_state: updated_app_state,
            creds: curr_state.creds.clone(),
            vault: maybe_vault,
            member: Some(membership.value),
        }
    }

    #[instrument(skip_all)]
    async fn join_cluster(&self, curr_state: ConfiguredAppState) {
        info!("Registration: Join cluster");

        let device_log_service = PersistentDeviceLog {
            p_obj: self.persistent_obj.clone()
        };

        device_log_service.init(curr_state.creds.user()).await?;
    }
}

impl<Repo> MetaClient<Repo>
    where
        Repo: KvLogEventRepo,
{
    pub async fn cluster_distribution(&self, pass_id: &str, pass: &str, app_state: &MemberAppState) {
        info!("Cluster distribution. App state: {:?}", app_state);

        if let VaultInfo::Member { vault } = &app_state.vault_info {
            let distributor = MetaDistributor {
                persistent_obj: self.persistent_obj.clone(),
                vault: vault.clone(),
                user_creds: Arc::new(app_state.creds.clone()),
            };

            distributor.distribute(pass_id.to_string(), pass.to_string()).await;
        } else {
            error!("Password distribution is not available. The user is not a member of the vault");
        }
    }
}
