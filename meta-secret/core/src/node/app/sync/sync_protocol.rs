use std::time::Duration;
use crate::node::api::{DataSyncResponse, SyncRequest};
use anyhow::Result;
use reqwest::Client;
use serde_json;

pub trait SyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse>;
}

pub struct HttpSyncProtocol {
}

impl SyncProtocol for HttpSyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let client = Client::new();
        let url = "http://192.168.0.112:3000/meta_request";
        
        let request_json = serde_json::to_string_pretty(&request).unwrap();
        tracing::info!("Отправляемый JSON: {}", request_json);
        
        let response = client
            .post(url)
            .timeout(Duration::from_secs(5))
            .header("Content-Type", "application/json")
            // .header("Access-Control-Allow-Origin", url)
            .json(&request)
            .send()
            .await?;
        
        tracing::info!("Статус ответа: {:?}", response.status());
        
        let result: DataSyncResponse = response.json().await?;
        Ok(result)

    }
}