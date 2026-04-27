use anyhow::{Result, bail};
use std::sync::Arc;

use crate::crypto::keys::TransportSk;
use crate::node::app::meta_app::meta_client_service::{
    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::app::sync::sync_protocol::{HttpSyncProtocol, SyncProtocol};
use crate::node::common::data_transfer::MpscDataTransfer;
use crate::node::common::model::device::common::{DeviceName, DeviceType};
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::ClaimId;
use crate::node::common::model::user::common::UserDataOutsiderStatus;
use crate::node::common::model::user::user_creds::UserCreds;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::{ApplicationState, VaultFullInfo};
use crate::node::db::actions::recover::RecoveryHandler;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::repo::persistent_credentials::PersistentCredentials;
use crate::secret::shared_secret::PlainText;

pub fn resolve_signup_vault_name(state: &ApplicationState) -> Result<VaultName> {
    match state {
        ApplicationState::Local(_) => {
            bail!("Sign up is not allowed in local state");
        }
        ApplicationState::Vault(vault_info) => match vault_info {
            VaultFullInfo::NotExists(user) => Ok(user.vault_name.clone()),
            VaultFullInfo::Outsider(outsider) => match outsider.status {
                UserDataOutsiderStatus::NonMember => Ok(outsider.user_data.vault_name.clone()),
                UserDataOutsiderStatus::Pending => {
                    bail!("Sign up is not allowed in pending state");
                }
                UserDataOutsiderStatus::Declined => {
                    bail!("Sign up is not allowed in declined state");
                }
            },
            VaultFullInfo::Member(_) => {
                bail!("Sign up is not allowed in vault state");
            }
        },
    }
}

pub fn find_recovery_claim_id_from_state(
    state: &ApplicationState,
    pass_id: &MetaPasswordId,
) -> Option<ClaimId> {
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
        return None;
    };

    member.ss_claims.find_recovery_claim_id(pass_id)
}

pub async fn recover_plain_text<Repo: KvLogEventRepo, SyncP: SyncProtocol>(
    sync_gateway: &SyncGateway<Repo, SyncP>,
    user_creds: UserCreds,
    claim_id: ClaimId,
    pass_id: MetaPasswordId,
) -> Result<PlainText> {
    let recovery_handler = RecoveryHandler {
        p_obj: sync_gateway.p_obj.clone(),
    };

    recovery_handler
        .recover(user_creds, claim_id, pass_id)
        .await
}

pub async fn build_client_components<Repo: KvLogEventRepo>(
    client_repo: Arc<Repo>,
    sync_protocol: Arc<HttpSyncProtocol>,
    master_key: TransportSk,
    device_name: DeviceName,
    device_type: DeviceType,
) -> Result<(
    Arc<SyncGateway<Repo, HttpSyncProtocol>>,
    Arc<MetaClientService<Repo, HttpSyncProtocol>>,
)> {
    let p_obj = Arc::new(PersistentObject::new(client_repo));

    let creds_repo = PersistentCredentials {
        p_obj: p_obj.clone(),
        master_key: master_key.clone(),
    };
    let device_creds = Arc::new(
        creds_repo
            .get_or_generate_device_creds_with_type(device_name, device_type)
            .await?,
    );

    let sync_gateway = Arc::new(SyncGateway {
        id: String::from("client-gateway"),
        p_obj: p_obj.clone(),
        sync: sync_protocol,
        master_key: master_key.clone(),
    });

    let state_provider = Arc::new(MetaClientStateProvider::new());

    let meta_client_service = Arc::new(MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MpscDataTransfer::new(),
        }),
        sync_gateway: sync_gateway.clone(),
        state_provider,
        p_obj,
        device_data: device_creds.device.clone(),
        master_key,
    });

    Ok((sync_gateway, meta_client_service))
}
