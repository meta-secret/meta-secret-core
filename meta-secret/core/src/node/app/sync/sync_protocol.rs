use crate::node::api::{DataSyncResponse, SyncRequest};
use anyhow::Result;
use reqwest::Client;

pub trait SyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse>;
}

pub struct HttpSyncProtocol {
}

impl SyncProtocol for HttpSyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let client = Client::new();
        let url = "http://192.168.0.112:3000/meta_request";
        
        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", url)
            .json(&request)
            .send()
            .await?;
        
        let result: DataSyncResponse = response.json().await?;
        Ok(result)

    }
}