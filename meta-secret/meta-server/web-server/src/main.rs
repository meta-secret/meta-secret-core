// Keep colocated endpoint tests below the production handlers.
#![allow(clippy::items_after_test_module)]

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router, routing::post};
use futures_util::StreamExt;
use futures_util::stream::{self, Stream};
use http::{StatusCode, Uri};
use meta_secret_core::node::common::model::vault::vault::VaultName;
use meta_server_node::server::state_invalidation::{StateInvalidation, StateInvalidationPublisher};
use serde_derive::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use anyhow::Result;
use axum::response::Html;
use axum::routing::get;
use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
use meta_secret_core::crypto::key_utils;
use meta_secret_core::node::api::{DataSyncResponse, SyncRequest};
use meta_secret_core::node::join_events::{JoinResultStatus, JoinResultSubscription};
use meta_server_node::server::server_app::{MetaServerDataTransfer, ServerApp};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{Level, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Clone)]
pub struct MetaServerAppState {
    data_transfer: Arc<MetaServerDataTransfer>,
    invalidation_tx: broadcast::Sender<StateInvalidation>,
    e2e_signals_enabled: bool,
}

#[derive(Clone)]
struct BroadcastStateInvalidationPublisher {
    tx: broadcast::Sender<StateInvalidation>,
}

impl StateInvalidationPublisher for BroadcastStateInvalidationPublisher {
    fn publish(&self, invalidation: StateInvalidation) {
        info!(
            vault_name = %invalidation.vault_name,
            scope = ?invalidation.scope,
            revision = ?invalidation.revision,
            "Publishing state invalidation"
        );
        let _ = self.tx.send(invalidation);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateEventsQuery {
    vault_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("debug"))
        .add_directive("hyper=info".parse()?)
        .add_directive("h2=info".parse()?)
        .add_directive("tower=info".parse()?)
        .add_directive("sqlx=info".parse()?);

    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // Use a more compact, abbreviated log format
        .compact()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::DEBUG)
        .with_env_filter(filter)
        // completes the builder.
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Server...");

    // Load or create a master key from a file
    let master_key_path = "master_key.json";
    let master_key = key_utils::load_or_create_master_key(master_key_path)?;
    info!("Master key loaded successfully");

    info!("Creating router...");
    let cors = CorsLayer::permissive();

    let (invalidation_tx, _) = broadcast::channel(1024);
    let invalidation_publisher = Arc::new(BroadcastStateInvalidationPublisher {
        tx: invalidation_tx.clone(),
    });

    let server_app = {
        let repo = Arc::new(SqlIteRepo {
            conn_url: String::from("file:meta-secret.db"),
        });
        Arc::new(ServerApp::new_with_invalidation_publisher(
            repo.clone(),
            master_key,
            invalidation_publisher,
        )?)
    };

    let data_transfer = server_app.get_data_transfer();
    let server_app_clone = server_app.clone();

    // Create a separate runtime for the server app
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            if let Err(e) = server_app_clone.run().await {
                panic!("Server app background task failed: {:?}", e);
            }
        });
    });

    let app_state = Arc::new(MetaServerAppState {
        data_transfer,
        invalidation_tx,
        e2e_signals_enabled: matches!(std::env::var("META_SERVER_E2E_SIGNALS").as_deref(), Ok("1")),
    });

    info!("Creating router...");
    let app = Router::new()
        .route("/meta_request", post(meta_request))
        .route("/state-events", get(state_events))
        .route("/join-events", get(join_events))
        .route("/hi", get(hi))
        .with_state(app_state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .fallback(not_found_handler);

    let port = 3000;
    info!("Run axum server, on port: {}", port);
    let listener = TcpListener::bind(format!("0.0.0.0:{:?}", port)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn state_events(
    State(state): State<Arc<MetaServerAppState>>,
    headers: HeaderMap,
    Query(query): Query<StateEventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    let subscription =
        parse_state_events_subscription(&headers, &VaultName::from(query.vault_name.clone()))?;
    let subscriber_device_id = subscription.signer.clone();
    let authorization_response = state
        .data_transfer
        .send_request(meta_secret_core::node::api::SyncRequest::Read(Box::new(
            meta_secret_core::node::api::ReadSyncRequest::StateEventsSubscription(subscription),
        )))
        .await
        .map_err(|_| {
            error_response(
                StatusCode::FORBIDDEN,
                "State events subscription is not authorized",
            )
        })?;
    authorization_response.ensure_success().map_err(|_| {
        error_response(
            StatusCode::FORBIDDEN,
            "State events subscription is not authorized",
        )
    })?;

    let vault_name = VaultName::from(query.vault_name);
    info!(
        vault_name = %vault_name,
        subscriber_device_id = %subscriber_device_id,
        "SSE client connected"
    );
    let rx = state.invalidation_tx.subscribe();
    let e2e_signals_enabled = state.e2e_signals_enabled;

    let stream = stream::unfold((rx, vault_name), move |(mut rx, vault_name)| async move {
        loop {
            match rx.recv().await {
                Ok(invalidation) if invalidation.vault_name == vault_name => {
                    if invalidation.event_type != "state_invalidated" {
                        continue;
                    }
                    let mut invalidation = invalidation;
                    if !e2e_signals_enabled {
                        invalidation.e2e_signal = None;
                    }
                    let data = match serde_json::to_string(&invalidation) {
                        Ok(data) => data,
                        Err(error) => {
                            tracing::warn!(
                                vault_name = %invalidation.vault_name,
                                error = ?error,
                                "Failed to serialize state invalidation"
                            );
                            continue;
                        }
                    };
                    let event = Event::default().event("state_invalidated").data(data);
                    return Some((Ok(event), (rx, vault_name)));
                }
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "SSE state invalidation receiver lagged");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

async fn join_events(
    State(state): State<Arc<MetaServerAppState>>,
    headers: HeaderMap,
    Query(query): Query<StateEventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    let subscription =
        parse_join_result_subscription(&headers, &VaultName::from(query.vault_name.clone()))?;
    let request_id = subscription.request_id.clone();
    // Subscribe before the canonical server check so an approval between the
    // check and stream creation remains buffered for delivery.
    let rx = state.invalidation_tx.subscribe();
    let authorization_response = state
        .data_transfer
        .send_request(SyncRequest::Read(Box::new(
            meta_secret_core::node::api::ReadSyncRequest::JoinResultSubscription(subscription),
        )))
        .await
        .map_err(|_| {
            error_response(
                StatusCode::FORBIDDEN,
                "JOIN result subscription is not authorized",
            )
        })?;
    let current_status = authorization_response.to_join_result().map_err(|_| {
        error_response(
            StatusCode::FORBIDDEN,
            "JOIN result subscription is not authorized",
        )
    })?;
    let vault_name = VaultName::from(query.vault_name);
    let stream =
        join_result_status_stream(Some(current_status), rx, vault_name, request_id.clone()).map(
            move |status| {
                let outcome = match status {
                    JoinResultStatus::Accepted => "accepted",
                    JoinResultStatus::Declined => "declined",
                    JoinResultStatus::Pending => unreachable!("pending is not a terminal event"),
                };
                Ok(join_result_event(&request_id, outcome))
            },
        );

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

fn join_result_event(request_id: &str, outcome: &str) -> Event {
    Event::default()
        .event("join_result")
        .data(serde_json::json!({ "requestId": request_id, "outcome": outcome }).to_string())
}

fn join_result_status_stream(
    initial_status: Option<JoinResultStatus>,
    rx: broadcast::Receiver<StateInvalidation>,
    vault_name: VaultName,
    request_id: String,
) -> impl Stream<Item = JoinResultStatus> {
    stream::unfold(
        (initial_status, rx, vault_name, request_id),
        |(status, mut rx, vault_name, request_id)| async move {
            if let Some(status) = status {
                match status {
                    JoinResultStatus::Accepted | JoinResultStatus::Declined => {
                        return Some((status, (None, rx, vault_name, request_id)));
                    }
                    JoinResultStatus::Pending => {}
                }
            } else {
                return None;
            }

            loop {
                match rx.recv().await {
                    Ok(invalidation)
                        if matches_join_result_invalidation(
                            &invalidation,
                            &vault_name,
                            &request_id,
                        ) =>
                    {
                        let status = match invalidation.join_result {
                            Some(
                                meta_server_node::server::state_invalidation::JoinResultOutcome::Accepted,
                            ) => JoinResultStatus::Accepted,
                            Some(
                                meta_server_node::server::state_invalidation::JoinResultOutcome::Declined,
                            ) => JoinResultStatus::Declined,
                            None => continue,
                        };
                        return Some((status, (None, rx, vault_name, request_id)));
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "JOIN result receiver lagged");
                        // A terminal decision may have been among the
                        // dropped messages. Force the client to reconnect;
                        // its fresh authorization reads canonical status.
                        return None;
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    )
}

fn matches_join_result_invalidation(
    invalidation: &meta_server_node::server::state_invalidation::StateInvalidation,
    vault_name: &VaultName,
    request_id: &str,
) -> bool {
    invalidation.event_type == "join_result"
        && invalidation.vault_name == *vault_name
        && invalidation.join_request_id.as_deref() == Some(request_id)
        && invalidation.join_result.is_some()
}

fn parse_state_events_subscription(
    headers: &HeaderMap,
    requested_vault: &VaultName,
) -> Result<
    meta_secret_core::node::state_events::StateEventsSubscription,
    (StatusCode, Json<ErrorResponse>),
> {
    let authorization = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            error_response(
                StatusCode::UNAUTHORIZED,
                "Missing state events authorization",
            )
        })?;
    let subscription =
        meta_secret_core::node::state_events::StateEventsSubscription::from_bearer_token(
            authorization,
        )
        .map_err(|_| {
            error_response(
                StatusCode::UNAUTHORIZED,
                "Invalid state events authorization",
            )
        })?;
    if subscription.vault_name != *requested_vault {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            "State events Vault does not match authorization",
        ));
    }
    Ok(subscription)
}

fn parse_join_result_subscription(
    headers: &HeaderMap,
    requested_vault: &VaultName,
) -> Result<JoinResultSubscription, (StatusCode, Json<ErrorResponse>)> {
    let authorization = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            error_response(
                StatusCode::UNAUTHORIZED,
                "Missing JOIN result authorization",
            )
        })?;
    let subscription = JoinResultSubscription::from_bearer_token(authorization).map_err(|_| {
        error_response(
            StatusCode::UNAUTHORIZED,
            "Invalid JOIN result authorization",
        )
    })?;
    if subscription.vault_name != *requested_vault {
        return Err(error_response(
            StatusCode::FORBIDDEN,
            "JOIN result Vault does not match authorization",
        ));
    }
    Ok(subscription)
}

async fn hi() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

fn error_response(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            message: message.to_string(),
        }),
    )
}
async fn not_found_handler(uri: Uri) -> (StatusCode, Json<ErrorResponse>) {
    let error_response = ErrorResponse {
        message: format!("404. MetaServer has no route: {uri}"),
    };
    let response = Json(error_response);
    (StatusCode::NOT_FOUND, response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use meta_secret_core::crypto::key_pair::{DsaKeyPair, KeyPair};
    use meta_secret_core::crypto::utils::U64IdUrlEnc;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::node::common::model::device::common::DeviceId;
    use meta_secret_core::node::db::events::vault::vault_log_event::JoinClusterEvent;
    use meta_secret_core::node::join_events::{JoinResultStatus, JoinResultSubscription};
    use meta_secret_core::node::state_events::StateEventsSubscription;
    use meta_server_node::server::state_invalidation::{
        JoinResultOutcome, StateInvalidation, StateInvalidationScope,
    };

    fn token(vault_name: &str) -> String {
        let key = DsaKeyPair::generate();
        StateEventsSubscription::sign_at(
            VaultName::from(vault_name),
            DeviceId(U64IdUrlEnc::from("web-state-events-test".to_string())),
            &key,
            1_000,
        )
        .unwrap()
        .bearer_token()
        .unwrap()
    }

    #[test]
    fn state_events_requires_authorization_header() {
        let headers = HeaderMap::new();
        let error =
            parse_state_events_subscription(&headers, &VaultName::from("vault-a")).unwrap_err();
        assert_eq!(error.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn state_events_rejects_invalid_or_cross_vault_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            "Bearer not-a-token".parse().unwrap(),
        );
        assert_eq!(
            parse_state_events_subscription(&headers, &VaultName::from("vault-a"))
                .unwrap_err()
                .0,
            StatusCode::UNAUTHORIZED
        );

        headers.insert(
            http::header::AUTHORIZATION,
            format!("Bearer {}", token("vault-a")).parse().unwrap(),
        );
        assert_eq!(
            parse_state_events_subscription(&headers, &VaultName::from("vault-b"))
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn join_result_requires_authorization_header() {
        let error = parse_join_result_subscription(&HeaderMap::new(), &VaultName::from("vault-a"))
            .unwrap_err();
        assert_eq!(error.0, StatusCode::UNAUTHORIZED);

        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            "Bearer not-a-token".parse().unwrap(),
        );
        assert_eq!(
            parse_join_result_subscription(&headers, &VaultName::from("vault-a"))
                .unwrap_err()
                .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn join_result_rejects_token_for_a_different_vault() {
        let registry = FixtureRegistry::empty();
        let candidate = registry.state.user_creds.vd.user();
        let request = JoinClusterEvent::from(candidate);
        let key_pair = registry
            .state
            .user_creds
            .vd
            .device_creds
            .key_manager()
            .unwrap()
            .dsa;
        let token = JoinResultSubscription::sign_at(&request, &key_pair, 1_000)
            .unwrap()
            .bearer_token()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );

        assert_eq!(
            parse_join_result_subscription(&headers, &VaultName::from("another-vault"))
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn join_result_stream_filters_other_and_stale_request_ids() {
        let vault = VaultName::from("vault-a");
        let current = StateInvalidation::join_result(
            vault.clone(),
            "request-current".into(),
            JoinResultOutcome::Accepted,
        );
        let stale = StateInvalidation::join_result(
            vault.clone(),
            "request-old".into(),
            JoinResultOutcome::Accepted,
        );
        let state_refresh = StateInvalidation::new(vault.clone(), StateInvalidationScope::Vault);

        assert!(matches_join_result_invalidation(
            &current,
            &vault,
            "request-current"
        ));
        assert!(!matches_join_result_invalidation(
            &stale,
            &vault,
            "request-current"
        ));
        assert!(!matches_join_result_invalidation(
            &state_refresh,
            &vault,
            "request-current"
        ));
    }

    #[test]
    fn join_result_listener_buffers_decision_published_during_authorization() {
        let vault = VaultName::from("vault-a");
        let (tx, _) = tokio::sync::broadcast::channel(1);
        // This is the ordering used by join_events: subscribe first, then ask
        // Core for canonical status. A terminal event racing that read remains
        // buffered and can be delivered after Core responds Pending.
        let mut receiver = tx.subscribe();
        tx.send(StateInvalidation::join_result(
            vault.clone(),
            "request-racing".into(),
            JoinResultOutcome::Declined,
        ))
        .unwrap();
        let event = receiver.try_recv().unwrap();
        assert!(matches_join_result_invalidation(
            &event,
            &vault,
            "request-racing"
        ));
        assert_eq!(event.join_result, Some(JoinResultOutcome::Declined));
    }

    #[tokio::test]
    async fn join_result_status_stream_emits_reconnected_terminal_result_once() {
        let vault = VaultName::from("vault-a");
        let (tx, rx) = tokio::sync::broadcast::channel(4);
        let accepted = join_result_status_stream(
            Some(JoinResultStatus::Accepted),
            rx,
            vault.clone(),
            "request-accepted".into(),
        );
        futures_util::pin_mut!(accepted);
        assert_eq!(accepted.next().await, Some(JoinResultStatus::Accepted));
        assert_eq!(accepted.next().await, None);

        let (_, rx) = tokio::sync::broadcast::channel(4);
        let declined = join_result_status_stream(
            Some(JoinResultStatus::Declined),
            rx,
            vault,
            "request-declined".into(),
        );
        futures_util::pin_mut!(declined);
        assert_eq!(declined.next().await, Some(JoinResultStatus::Declined));
        assert_eq!(declined.next().await, None);
        drop(tx);
    }

    #[tokio::test]
    async fn join_result_status_stream_waits_for_only_the_exact_pending_result_once() {
        let vault = VaultName::from("vault-a");
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let statuses = join_result_status_stream(
            Some(JoinResultStatus::Pending),
            rx,
            vault.clone(),
            "request-current".into(),
        );
        futures_util::pin_mut!(statuses);
        tx.send(StateInvalidation::new(
            vault.clone(),
            StateInvalidationScope::Vault,
        ))
        .unwrap();
        tx.send(StateInvalidation::join_result(
            VaultName::from("vault-b"),
            "request-current".into(),
            JoinResultOutcome::Accepted,
        ))
        .unwrap();
        tx.send(StateInvalidation::join_result(
            vault.clone(),
            "request-old".into(),
            JoinResultOutcome::Declined,
        ))
        .unwrap();
        tx.send(StateInvalidation::join_result(
            vault,
            "request-current".into(),
            JoinResultOutcome::Declined,
        ))
        .unwrap();

        assert_eq!(statuses.next().await, Some(JoinResultStatus::Declined));
        assert_eq!(statuses.next().await, None);
    }

    #[tokio::test]
    async fn join_result_status_stream_closes_after_lag_or_sender_close() {
        let vault = VaultName::from("vault-a");
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        let lagged = join_result_status_stream(
            Some(JoinResultStatus::Pending),
            rx,
            vault.clone(),
            "request-current".into(),
        );
        futures_util::pin_mut!(lagged);
        tx.send(StateInvalidation::join_result(
            vault.clone(),
            "request-current".into(),
            JoinResultOutcome::Accepted,
        ))
        .unwrap();
        tx.send(StateInvalidation::join_result(
            vault.clone(),
            "request-current".into(),
            JoinResultOutcome::Declined,
        ))
        .unwrap();
        assert_eq!(lagged.next().await, None);

        let (tx, rx) = tokio::sync::broadcast::channel(1);
        drop(tx);
        let closed = join_result_status_stream(
            Some(JoinResultStatus::Pending),
            rx,
            vault,
            "request-current".into(),
        );
        futures_util::pin_mut!(closed);
        assert_eq!(closed.next().await, None);
    }
}

pub async fn meta_request(
    State(state): State<Arc<MetaServerAppState>>,
    Json(msg_request): Json<SyncRequest>,
) -> Json<DataSyncResponse> {
    info!("Event processing");

    let response = state.data_transfer.send_request(msg_request).await.unwrap();

    Json(response)
}
