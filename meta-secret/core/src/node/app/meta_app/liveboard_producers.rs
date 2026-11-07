//! Producers use the same FIFO/private reply and owning operation guard as legacy.
use super::{
    messaging::{GenericAppStateRequest as Q, GenericAppStateResponse as A},
    meta_client_service::MetaClientService,
    producer_receipt,
};
use crate::node::{
    app::{orchestrator::MetaOrchestrator, sync::sync_protocol::SyncProtocol},
    common::model::{liveboard_producer::*, meta_pass::MetaPasswordId},
    db::{
        actions::secret_deletion::SecretDeletionAction,
        events::vault::vault_log_event::{VaultActionEvent, VaultActionRequestEvent},
        objects::{
            local_removal_envelope::PersistentRemovalEnvelope,
            persistent_device_log::PersistentDeviceLog, persistent_vault::PersistentVault,
            producer_append::LocalProducerAppend,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, bail, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> MetaClientService<R, S> {
    pub async fn resolve_producer_secret(&self, identifier: &str) -> Result<MetaPasswordId> {
        ensure!(
            !identifier.is_empty() && identifier.len() <= 512,
            "OperationUnavailable"
        );
        let creds = self.find_user_creds().await?;
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(creds.vault_name.clone())
            .await?
            .0
            .value;
        ensure!(vault.is_member(creds.device_id()), "OperationUnavailable");
        let matches: Vec<_> = vault
            .secrets
            .into_iter()
            .filter(|pass| pass.id_str() == identifier)
            .collect();
        let [pass] = matches.as_slice() else {
            bail!("OperationUnavailable");
        };
        Ok(pass.clone())
    }

    pub async fn send_producer_request(&self, request: Q) -> Result<LiveboardProducerReceipt> {
        ensure!(request.is_producer(), "OperationUnavailable");
        match self
            .data_transfer
            .dt
            .send_to_service_and_get(request)
            .await?
        {
            A::ProducerReceipt(receipt) => Ok(receipt),
            _ => bail!("OperationUnavailable"),
        }
    }

    pub(super) async fn execute_producer_request(&self, request: Q) -> A {
        let result = match request {
            Q::DeleteSecretChecked(pass) => self.delete_secret_checked(pass).await,
            Q::RemoveMemberChecked(target) => self.remove_member_checked(target).await,
            Q::ReconcileProducerReceipt(reference) => {
                self.reconcile_producer_receipt(reference).await
            }
            Q::ContinueLocalRemoveChecked(reference) => {
                self.continue_local_remove_checked(reference).await
            }
            _ => Err(anyhow::anyhow!("OperationUnavailable")),
        };
        match result {
            Ok(receipt) => A::ProducerReceipt(receipt),
            Err(_) => A::Error("OperationUnavailable".into()),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_secret_checked(
        &self,
        pass: MetaPasswordId,
    ) -> Result<LiveboardProducerReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self.find_user_creds().await?;
        self.sync_gateway.confirm_producer_support().await?;
        self.sync_gateway.sync(creds.user()).await?;
        ensure!(
            PersistentRemovalEnvelope {
                p_obj: self.p_obj.clone()
            }
            .producer_context(&creds.user().user_id())
            .await?
            .is_none(),
            "OperationUnavailable"
        );
        let request = SecretDeletionAction {
            p_obj: self.p_obj.clone(),
            user_creds: creds.clone(),
        }
        .build_request(pass)
        .await?;
        let append = PersistentDeviceLog::from(self.p_obj.clone())
            .append_producer(
                &creds.user(),
                VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request)),
                None,
            )
            .await?;
        let receipt = producer_receipt::attempted(&append.event, append.stage, None)?;
        if !append.persisted || self.sync_gateway.sync(creds.user()).await.is_err() {
            return Ok(receipt);
        }
        self.observe_producer_receipt(&creds, receipt).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_member_checked(&self, target: String) -> Result<LiveboardProducerReceipt> {
        let _owner = self.p_obj.client_operation_guard.lock().await;
        let creds = self.find_user_creds().await?;
        self.sync_gateway.confirm_producer_support().await?;
        self.sync_gateway.sync(creds.user()).await?;
        let registry = PersistentRemovalEnvelope {
            p_obj: self.p_obj.clone(),
        };
        ensure!(
            registry
                .producer_context(&creds.user().user_id())
                .await?
                .is_none(),
            "OperationUnavailable"
        );
        let vault = PersistentVault::from(self.p_obj.clone())
            .get_vault(creds.vault_name.clone())
            .await?;
        let matches: Vec<_> = vault
            .0
            .value
            .members()
            .into_iter()
            .filter(|m| m.user_data.device.device_id.to_string() == target)
            .collect();
        let [target] = matches.as_slice() else {
            bail!("OperationUnavailable");
        };
        let orchestrator = MetaOrchestrator {
            p_obj: self.p_obj.clone(),
            user_creds: creds.clone(),
        };
        let request = orchestrator
            .build_remove_member_request(target.clone())
            .await?;
        let local = (vault.0.value.members().len() == 2).then(|| LocalProducerAppend::Request {
            admission_vault: vault.0.key.obj_id.clone(),
            target: target.user_data.device.device_id.clone(),
        });
        let append = PersistentDeviceLog::from(self.p_obj.clone())
            .append_producer(
                &creds.user(),
                VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request.clone())),
                local,
            )
            .await?;
        let receipt =
            producer_receipt::attempted(&append.event, append.stage, append.context.as_ref())?;
        if !append.persisted {
            return Ok(receipt);
        }
        if let Some(context) = append.context {
            return Ok(self
                .finish_local_producer(&creds, request, context, receipt.clone())
                .await
                .unwrap_or(receipt));
        }
        if self.sync_gateway.sync(creds.user()).await.is_err() {
            return Ok(receipt);
        }
        self.observe_producer_receipt(&creds, receipt).await
    }
}
