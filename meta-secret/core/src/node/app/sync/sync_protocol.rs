use crate::node::api::{DataSyncResponse, SyncRequest};
use crate::node::app::sync::api_url::ApiUrl;
use anyhow::Result;
use reqwest::Client;
use std::time::Duration;

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

        let response = client
            .post(url.clone())
            .timeout(Duration::from_secs(3))
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", url)
            .json(&request)
            .send()
            .await?;

        let result: DataSyncResponse = response.json().await?;
        Ok(result)
    }
}
