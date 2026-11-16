use anyhow::{Result, bail, ensure};
use std::sync::Arc;

use crate::crypto::keys::TransportSk;
use crate::node::app::meta_app::client_transfer::MetaClientRequestTransfer;
use crate::node::app::meta_app::meta_client_service::{
    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use crate::node::app::sync::sync_gateway::SyncGateway;
use crate::node::app::sync::sync_protocol::{HttpSyncProtocol, SyncProtocol};
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
    let _owner = sync_gateway.p_obj.client_operation_guard.lock().await;
    ensure_current_recovery_session(sync_gateway, &user_creds).await?;
    let recovery_handler = RecoveryHandler {
        p_obj: sync_gateway.p_obj.clone(),
    };

    recovery_handler
        .recover(user_creds, claim_id, pass_id)
        .await
}

/// Recover a secret using the exact recovery claim selected by Core.
///
/// The pass ID is read from the persisted claim. Callers cannot pair a claim
/// with an unrelated secret, which keeps the recovery distribution and the
/// completion event bound to the same request.
pub async fn recover_plain_text_by_claim<Repo: KvLogEventRepo, SyncP: SyncProtocol>(
    sync_gateway: &SyncGateway<Repo, SyncP>,
    user_creds: UserCreds,
    claim_id: ClaimId,
) -> Result<PlainText> {
    let _owner = sync_gateway.p_obj.client_operation_guard.lock().await;
    ensure_current_recovery_session(sync_gateway, &user_creds).await?;
    let recovery_handler = RecoveryHandler {
        p_obj: sync_gateway.p_obj.clone(),
    };

    recovery_handler
        .recover_by_claim_id(user_creds, claim_id)
        .await
}

/// Called only while the shared recovery entry holds the client owner.
pub async fn recover_plain_text_by_claim_checked<Repo: KvLogEventRepo, SyncP: SyncProtocol>(
    sync_gateway: &SyncGateway<Repo, SyncP>,
    user_creds: UserCreds,
    claim_id: ClaimId,
) -> Result<crate::node::common::model::liveboard_reveal::RecoveredReveal> {
    let _owner = sync_gateway.p_obj.client_operation_guard.lock().await;
    ensure_current_recovery_session(sync_gateway, &user_creds).await?;
    RecoveryHandler {
        p_obj: sync_gateway.p_obj.clone(),
    }
    .recover_by_claim_id_checked(user_creds, claim_id)
    .await
}

/// Called only while the shared recovery entry holds the client owner.
async fn ensure_current_recovery_session<Repo: KvLogEventRepo, SyncP: SyncProtocol>(
    sync_gateway: &SyncGateway<Repo, SyncP>,
    expected: &UserCreds,
) -> Result<()> {
    let actual = PersistentCredentials {
        p_obj: sync_gateway.p_obj.clone(),
        master_key: sync_gateway.master_key.clone(),
    }
    .get_user_creds()
    .await?;
    ensure!(
        actual.as_ref() == Some(expected),
        "Recovery session is unavailable"
    );
    Ok(())
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

    let sync_gateway = Arc::new(SyncGateway::new(
        String::from("client-gateway"),
        p_obj.clone(),
        sync_protocol,
        master_key.clone(),
    ));

    let state_provider = Arc::new(MetaClientStateProvider::new());

    let meta_client_service = Arc::new(MetaClientService {
        data_transfer: Arc::new(MetaClientDataTransfer {
            dt: MetaClientRequestTransfer::new(),
        }),
        sync_gateway: sync_gateway.clone(),
        state_provider,
        p_obj,
        device_data: device_creds.device.clone(),
        master_key,
    });

    Ok((sync_gateway, meta_client_service))
}
