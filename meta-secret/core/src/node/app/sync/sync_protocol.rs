use crate::node::api::{DataSyncResponse, SyncRequest};
use crate::node::app::sync::api_url::ApiUrl;
use anyhow::Result;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::debug;

pub trait SyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse>;
}

pub struct HttpSyncProtocol {
    pub api_url: ApiUrl,
}

impl SyncProtocol for HttpSyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let client = Client::new();
        let url = self.api_url.get_url() + "/meta_request";
        let started_at = Instant::now();

        let response = client
            .post(url.clone())
            .timeout(Duration::from_secs(15))
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", url)
            .json(&request)
            .send()
            .await?;

        let http_elapsed_ms = started_at.elapsed().as_millis();
        debug!(
            http_elapsed_ms,
            "sync_protocol: meta_request http response received"
        );

        let deserialize_started_at = Instant::now();
        let result: DataSyncResponse = response.json().await?;
        debug!(
            deserialize_elapsed_ms = deserialize_started_at.elapsed().as_millis(),
            total_elapsed_ms = started_at.elapsed().as_millis(),
            "sync_protocol: meta_request response decoded"
        );
        Ok(result)
    }
}
