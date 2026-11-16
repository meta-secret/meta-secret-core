//! Checked operations share the service's single execution owner with MPSC commands.
use super::meta_client_service::MetaClientService;
use crate::node::api::CanonicalDecisionInput;
use crate::node::app::{orchestrator::MetaOrchestrator, sync::sync_protocol::SyncProtocol};
use crate::node::common::model::{
    liveboard::{LiveboardOperationKind, LiveboardOperationPhase, LiveboardOperationReceipt},
    meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
    secret::SecretDistributionType,
    vault::vault::VaultStatus,
};
use crate::node::db::{
    actions::{recover::RecoveryAction, sign_up::join::JoinActionUpdate},
    events::vault::vault_log_event::VaultActionRequestEvent,
    objects::{
        persistent_shared_secret::PersistentSharedSecret, persistent_vault::PersistentVault,
    },
    repo::generic_db::KvLogEventRepo,
};
use crate::secret::MetaDistributor;
use anyhow::{Result, bail, ensure};
use std::sync::Arc;

impl<R: KvLogEventRepo, S: SyncProtocol> MetaClientService<R, S> {
    pub async fn decide_join_request(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<LiveboardOperationReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        ensure!(
            uuid::Uuid::parse_str(&input.request_id).is_ok(),
            "InvalidRequestId"
        );
        let creds = self.find_user_creds().await?;
        let vault = PersistentVault::from(self.p_obj.clone());
        let log = vault
            .vault_log(creds.vault_name.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!("JoinNotPending"))?;
        let matches: Vec<_> = log
            .0
            .value
            .requests
            .iter()
            .filter_map(|r| match r {
                VaultActionRequestEvent::JoinCluster(j) if j.request_id == input.request_id => {
                    Some(j.clone())
                }
                _ => None,
            })
            .collect();
        let [request] = matches.as_slice() else {
            bail!("JoinNotPending")
        };
        self.sync_gateway
            .sync(creds.user())
            .await
            .map_err(|_| anyhow::anyhow!("SyncUnavailable"))?;
        let refreshed = vault
            .vault_log(creds.vault_name.clone())
            .await?
            .ok_or_else(|| anyhow::anyhow!("JoinNotPending"))?;
        ensure!(
            refreshed
                .0
                .value
                .requests
                .contains(&VaultActionRequestEvent::JoinCluster(request.clone())),
            "JoinNotPending"
        );
        let orchestrator = MetaOrchestrator {
            p_obj: self.p_obj.clone(),
            user_creds: creds,
        };
        orchestrator
            .update_membership(
                request.clone(),
                if input.accepted {
                    JoinActionUpdate::Accept
                } else {
                    JoinActionUpdate::Decline
                },
            )
            .await
            .map_err(|_| anyhow::anyhow!("JoinDecisionUnavailable"))?;
        let mut receipt = LiveboardOperationReceipt::queued(LiveboardOperationKind::Join);
        receipt.request_id = Some(request.request_id.clone());
        Ok(receipt)
    }

    pub async fn cluster_distribution_checked(
        &self,
        input: PlainPassInfo,
    ) -> Result<LiveboardOperationReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self
            .find_user_creds()
            .await
            .map_err(|_| anyhow::anyhow!("MemberUnavailable"))?;
        self.sync_gateway
            .sync(creds.user())
            .await
            .map_err(|_| anyhow::anyhow!("SyncUnavailable"))?;
        let vault = PersistentVault::from(self.p_obj.clone());
        let VaultStatus::Member(member) = vault.find(creds.user()).await? else {
            bail!("MemberUnavailable")
        };
        let vm = vault
            .get_vault(creds.vault_name.clone())
            .await?
            .to_data()
            .to_vault_member(member)?;
        let distributor = MetaDistributor {
            p_obj: self.p_obj.clone(),
            user_creds: Arc::new(creds.clone()),
            vault_member: vm.clone(),
        };
        let receipt = distributor
            .distribute_checked(vm, SecurePassInfo::from(input))
            .await
            .map_err(|_| anyhow::anyhow!("CreateNotQueued"))?;
        if receipt.error_code.is_some() {
            return Ok(receipt);
        }
        if self.sync_gateway.sync(creds.user()).await.is_err() {
            return Ok(receipt.unknown("SyncOutcomeUnknown"));
        }
        Ok(receipt)
    }

    pub async fn recover_checked(&self, pass: MetaPasswordId) -> Result<LiveboardOperationReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self
            .find_user_creds()
            .await
            .map_err(|_| anyhow::anyhow!("MemberUnavailable"))?;
        self.sync_gateway
            .sync(creds.user())
            .await
            .map_err(|_| anyhow::anyhow!("SyncUnavailable"))?;
        let receipt = RecoveryAction::from(self.p_obj.clone())
            .recovery_request_checked(creds.clone(), pass.clone())
            .await
            .map_err(|_| anyhow::anyhow!("RecoverNotQueued"))?;
        if receipt.phase == LiveboardOperationPhase::Unknown || receipt.error_code.is_some() {
            return Ok(receipt);
        }
        if self.sync_gateway.sync(creds.user()).await.is_err() {
            return Ok(receipt.unknown("SyncOutcomeUnknown"));
        }
        let confirmed = PersistentSharedSecret::from(self.p_obj.clone())
            .get_ss_log_obj(creds.vault_name.clone())
            .await
            .is_ok_and(|log| {
                receipt.claim_id.as_ref().is_some_and(|id| {
                    log.claims.iter().any(|(key, claim)| {
                        key.wasm_id_str() == *id
                            && claim.id == *key
                            && claim.dist_claim_id.id == *key
                            && claim.sender == *creds.device_id()
                            && claim.vault_name == creds.vault_name
                            && claim.distribution_type == SecretDistributionType::Recover
                            && claim.dist_claim_id.pass_id == pass
                    })
                })
            });
        if !confirmed {
            return Ok(receipt.unknown("RecoverCanonicalUnknown"));
        }
        Ok(receipt)
    }
}

impl<R: KvLogEventRepo, S: SyncProtocol> MetaClientService<R, S> {
    pub async fn send_checked_request(
        &self,
        request: super::messaging::GenericAppStateRequest,
    ) -> Result<LiveboardOperationReceipt> {
        ensure!(request.is_checked(), "CheckedRequestRequired");
        match self
            .data_transfer
            .dt
            .send_to_service_and_get(request)
            .await?
        {
            super::messaging::GenericAppStateResponse::OperationReceipt(receipt) => Ok(receipt),
            super::messaging::GenericAppStateResponse::Error(_) => bail!("OperationUnavailable"),
            _ => bail!("UnexpectedOperationResponse"),
        }
    }
    pub(super) async fn execute_checked_request(
        &self,
        request: super::messaging::GenericAppStateRequest,
    ) -> super::messaging::GenericAppStateResponse {
        use super::messaging::{GenericAppStateRequest as Q, GenericAppStateResponse as A};
        let result = match request {
            Q::ClusterDistributionChecked(input) => self.cluster_distribution_checked(input).await,
            Q::RecoverChecked(pass) => self.recover_checked(pass).await,
            Q::DecideJoinRequest(input) => self.decide_join_request(input).await,
            Q::DecideRemoveRequest(input) => self.decide_remove_request(input).await,
            _ => Err(anyhow::anyhow!("CheckedRequestRequired")),
        };
        match result {
            Ok(receipt) => A::OperationReceipt(receipt),
            Err(_) => A::Error("OperationUnavailable".into()),
        }
    }
    pub async fn decide_remove_request(
        &self,
        input: CanonicalDecisionInput,
    ) -> Result<LiveboardOperationReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self.find_user_creds().await?;
        let actual = self.sync_gateway.decide_remove_member(input).await?;
        let mut receipt = LiveboardOperationReceipt::queued(LiveboardOperationKind::RemoveDevice);
        receipt.request_id = Some(actual.request_id);
        receipt.phase = actual.phase;
        receipt.error_code = actual.error_code;
        if receipt.error_code.is_some() {
            return Ok(receipt);
        }
        if self.sync_gateway.sync(creds.user()).await.is_err() {
            return Ok(receipt.unknown("SyncOutcomeUnknown"));
        }
        Ok(receipt)
    }
}
