use crate::node::api::{DataSyncResponse, SyncRequest};
use crate::node::app::sync::api_url::ApiUrl;
use anyhow::Result;
use async_std::sync::Mutex;
use reqwest::Client;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use tracing::debug;

pub trait SyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse>;
}

pub struct HttpSyncProtocol {
    pub api_url: ApiUrl,
    /// One in-flight HTTP sync at a time; concurrent browser fetches can abort body reads (AbortError).
    send_mutex: Mutex<()>,
}

impl HttpSyncProtocol {
    pub fn new(api_url: ApiUrl) -> Self {
        Self {
            api_url,
            send_mutex: Mutex::new(()),
        }
    }
}

/// Browser clients (CORS, TLS, mobile networks) often exceed a short desktop timeout.
fn meta_request_timeout() -> Duration {
    #[cfg(target_arch = "wasm32")]
    {
        Duration::from_secs(60)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Duration::from_secs(15)
    }
}

impl SyncProtocol for HttpSyncProtocol {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        let _send_guard = self.send_mutex.lock().await;

        let client = Client::new();
        let url = self.api_url.get_url() + "/meta_request";
        #[cfg(not(target_arch = "wasm32"))]
        let started_at = Instant::now();

        let response = client
            .post(url.clone())
            .timeout(meta_request_timeout())
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let http_elapsed_ms = started_at.elapsed().as_millis();
            debug!(
                http_elapsed_ms,
                "sync_protocol: meta_request http response received"
            );
        }
        #[cfg(target_arch = "wasm32")]
        debug!("sync_protocol: meta_request http response received");

        #[cfg(not(target_arch = "wasm32"))]
        let deserialize_started_at = Instant::now();
        let result: DataSyncResponse = response.json().await?;
        #[cfg(not(target_arch = "wasm32"))]
        debug!(
            deserialize_elapsed_ms = deserialize_started_at.elapsed().as_millis(),
            total_elapsed_ms = started_at.elapsed().as_millis(),
            "sync_protocol: meta_request response decoded"
        );
        #[cfg(target_arch = "wasm32")]
        debug!("sync_protocol: meta_request response decoded");
        Ok(result)
    }
}
