//! WebSocket `/meta_ws` — push server [`DataSyncResponse`] JSON (same as HTTP `/meta_request` body shape).
//! **Origin policy (browser):** optional allowlist via `META_WS_ALLOWED_ORIGINS`. Native clients often
//! omit `Origin`; see `META_WS_ALLOW_NO_ORIGIN`.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::header::ORIGIN;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use meta_secret_core::node::api::DataSyncResponse;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tracing::{error, info, warn};

use crate::MetaServerAppState;

/// Parsed once at startup from environment (see [`meta_ws_origin_config_from_env`]).
#[derive(Clone, Debug)]
pub struct MetaWsOriginConfig {
    pub allowed_origins: Vec<String>,
    pub allow_no_origin: bool,
}

pub fn meta_ws_origin_config_from_env() -> MetaWsOriginConfig {
    let allowed_origins = std::env::var("META_WS_ALLOWED_ORIGINS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let allow_no_origin = std::env::var("META_WS_ALLOW_NO_ORIGIN")
        .map(|v| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true);

    MetaWsOriginConfig {
        allowed_origins,
        allow_no_origin,
    }
}

fn origin_request_allowed(headers: &HeaderMap, policy: &MetaWsOriginConfig) -> bool {
    if policy.allowed_origins.is_empty() {
        return true;
    }

    match headers.get(ORIGIN) {
        None => policy.allow_no_origin,
        Some(value) => match value.to_str() {
            Ok(origin) => policy
                .allowed_origins
                .iter()
                .any(|allowed| allowed == origin),
            Err(_) => false,
        },
    }
}

pub async fn meta_ws_handler(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<Arc<MetaServerAppState>>,
) -> impl IntoResponse {
    if !origin_request_allowed(&headers, &state.meta_ws_origin) {
        warn!("meta_ws: rejected WebSocket upgrade (Origin policy)");
        return (
            StatusCode::FORBIDDEN,
            "meta_ws: origin not allowed",
        )
            .into_response();
    }

    let rx = state.data_transfer.subscribe_sync_events();
    ws.on_upgrade(move |socket| handle_meta_ws(socket, rx))
}

async fn handle_meta_ws(mut socket: WebSocket, mut rx: tokio::sync::broadcast::Receiver<DataSyncResponse>) {
    info!("WebSocket /meta_ws connected");
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(resp) => {
                        match serde_json::to_string(&resp) {
                            Ok(json) => {
                                if socket.send(Message::text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => error!("meta_ws serialize: {:?}", e),
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        warn!("meta_ws subscriber lagged, skipped {} messages", n);
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    None => break,
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Ping(p))) => {
                        let _ = socket.send(Message::Pong(p)).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
    info!("WebSocket /meta_ws closed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn empty_allowlist_allows_anything() {
        let policy = MetaWsOriginConfig {
            allowed_origins: vec![],
            allow_no_origin: false,
        };
        let mut h = HeaderMap::new();
        assert!(origin_request_allowed(&h, &policy));
        h.insert(ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(origin_request_allowed(&h, &policy));
    }

    #[test]
    fn allowlist_requires_match_when_origin_present() {
        let policy = MetaWsOriginConfig {
            allowed_origins: vec!["http://127.0.0.1:5173".to_string()],
            allow_no_origin: false,
        };
        let mut h = HeaderMap::new();
        assert!(!origin_request_allowed(&h, &policy));
        h.insert(ORIGIN, HeaderValue::from_static("http://127.0.0.1:5173"));
        assert!(origin_request_allowed(&h, &policy));
        h.insert(ORIGIN, HeaderValue::from_static("https://other"));
        assert!(!origin_request_allowed(&h, &policy));
    }

    #[test]
    fn allow_no_origin_when_configured() {
        let policy = MetaWsOriginConfig {
            allowed_origins: vec!["http://a".to_string()],
            allow_no_origin: true,
        };
        let h = HeaderMap::new();
        assert!(origin_request_allowed(&h, &policy));
    }

    #[test]
    fn non_utf8_origin_rejected_when_allowlist_nonempty() {
        let policy = MetaWsOriginConfig {
            allowed_origins: vec!["http://127.0.0.1:5173".to_string()],
            allow_no_origin: false,
        };
        let mut h = HeaderMap::new();
        let hv = HeaderValue::from_bytes(&[0x80])
            .expect("single obs-text byte must be a valid header value for this regression");
        assert!(
            hv.to_str().is_err(),
            "http::HeaderValue::to_str must fail for non-visible bytes"
        );
        h.insert(ORIGIN, hv);
        assert!(!origin_request_allowed(&h, &policy));
    }
}
