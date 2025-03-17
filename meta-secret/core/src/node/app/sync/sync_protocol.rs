use crate::node::api::{DataSyncResponse, SyncRequest};
use anyhow::Result;

pub trait SyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse>;
}
