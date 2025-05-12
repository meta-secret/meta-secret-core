use axum::extract::State;
use axum::{routing::post, Json, Router};
use http::{StatusCode, Uri};
use serde_derive::Serialize;
use std::sync::Arc;

use anyhow::Result;
use axum::response::Html;
use axum::routing::get;
use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
use meta_secret_core::node::api::{DataSyncResponse, SyncRequest};
use meta_server_node::server::server_app::{MetaServerDataTransfer, ServerApp};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Clone)]
pub struct MetaServerAppState {
    data_transfer: Arc<MetaServerDataTransfer>,
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

    info!("Creating router...");
    let cors = CorsLayer::permissive();

    let server_app = {
        let repo = Arc::new(SqlIteRepo {
            conn_url: String::from("file:meta-secret.db"),
        });
        Arc::new(ServerApp::new(repo.clone())?)
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

    let app_state = Arc::new(MetaServerAppState { data_transfer });

    info!("Creating router...");
    let app = Router::new()
        .route("/meta_request", post(meta_request))
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
