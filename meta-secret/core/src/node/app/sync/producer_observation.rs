//! A producer observation is one ordered read, never a client synchronization.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    api::{
        DataSyncResponse, LiveboardCapabilitiesRequest, ReadSyncRequest, ServerTailRequest,
        SyncRequest,
    },
    common::model::{
        liveboard_producer::{ProducerCanonicalObservation, ProducerReference},
        user::common::UserData,
    },
    db::repo::generic_db::KvLogEventRepo,
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    pub(crate) async fn confirm_producer_support(&self) -> Result<()> {
        let response = self
            .sync
            .send(SyncRequest::Read(Box::new(
                ReadSyncRequest::LiveboardCapabilities(LiveboardCapabilitiesRequest { version: 1 }),
            )))
            .await?;
        ensure!(
            matches!(response, DataSyncResponse::LiveboardCapabilities(c) if c.producer_correlation_version == 1),
            "OperationUnavailable"
        );
        Ok(())
    }

    pub(crate) async fn observe_producer(
        &self,
        user: UserData,
        reference: ProducerReference,
    ) -> Result<Option<ProducerCanonicalObservation>> {
        let _owner = self.sync_guard.lock().await;
        let response = self
            .sync
            .send(SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(
                ServerTailRequest {
                    sender: user,
                    producer_reference: Some(reference),
                },
            ))))
            .await?
            .to_server_tail()?;
        Ok(response.producer_observation)
    }
}
