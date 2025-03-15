use axum::extract::State;
use axum::{
    routing::{get, post},
    Json, Router,
};
use http::{StatusCode, Uri};
use serde_derive::{Deserialize, Serialize};

use meta_secret_core::node::api::SyncRequest;
use meta_secret_core::node::server::server_data_sync::{DataSyncResponse, ServerTailResponse};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Debug, Clone, Deserialize)]
pub struct MetaServerAppState {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    info!("Creating router...");
    let cors = CorsLayer::permissive();

    let app_state = MetaServerAppState {};

    info!("Creating router...");
    let app = Router::new()
        .route("/event", post(event_processing))
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

pub async fn event_processing(
    State(state): State<MetaServerAppState>,
    Json(msg_request): Json<SyncRequest>,
) -> Result<Json<DataSyncResponse>, StatusCode> {
    info!("Event processing");

    let resp = ServerTailResponse {
        device_log_tail: None,
        ss_device_log_tail: None,
    };
    Ok(Json(DataSyncResponse::ServerTailResponse(resp)))
}
