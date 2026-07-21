use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router, routing::post};
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
    });

    info!("Creating router...");
    let app = Router::new()
        .route("/meta_request", post(meta_request))
        .route("/state-events", get(state_events))
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
    Query(query): Query<StateEventsQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let vault_name = VaultName::from(query.vault_name);
    info!(vault_name = %vault_name, "SSE client connected");
    let rx = state.invalidation_tx.subscribe();

    let stream = stream::unfold((rx, vault_name), |(mut rx, vault_name)| async move {
        loop {
            match rx.recv().await {
                Ok(invalidation) if invalidation.vault_name == vault_name => {
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

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

async fn hi() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}
async fn not_found_handler(uri: Uri) -> (StatusCode, Json<ErrorResponse>) {
    let error_response = ErrorResponse {
        message: format!("404. MetaServer has no route: {uri}"),
    };
    let response = Json(error_response);
    (StatusCode::NOT_FOUND, response)
}

pub async fn meta_request(
    State(state): State<Arc<MetaServerAppState>>,
    Json(msg_request): Json<SyncRequest>,
) -> Json<DataSyncResponse> {
    info!("Event processing");

    let response = state.data_transfer.send_request(msg_request).await.unwrap();

    Json(response)
}
