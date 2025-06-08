mod handler;
mod notification_service;

use axum::extract::State;
use axum::{Json, Router, routing::post};
use http::{StatusCode, Uri};
use serde_derive::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::handler::ws_echo::ws_echo_handler;
use crate::notification_service::NotificationServiceInterface;
use anyhow::Result;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::Html;
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{StreamExt};
use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
use meta_secret_core::crypto::key_utils;
use meta_secret_core::node::api::{DataSyncResponse, SyncRequest};
use meta_secret_core::node::common::model::user::common::UserId;
use meta_server_node::server::server_app::{MetaServerDataTransfer, ServerApp};
use notification_service::{NotificationRequest, NotificationService};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{Level, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Clone)]
pub struct MetaServerAppState {
    data_transfer: Arc<MetaServerDataTransfer>,
    notification_service: Arc<NotificationServiceInterface>,
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

    let server_app = {
        let repo = Arc::new(SqlIteRepo {
            conn_url: String::from("file:meta-secret.db"),
        });
        Arc::new(ServerApp::new(repo.clone(), master_key)?)
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

    let notification_service = Arc::new(NotificationService::run().await);
    let app_state = Arc::new(MetaServerAppState {
        data_transfer,
        notification_service,
    });

    info!("Creating router...");
    let app = Router::new()
        .route("/meta_request", post(meta_request))
        .route("/hi", get(hi))
        .route("/ws_echo", get(ws_echo_handler))
        .route("/ws_subscribe", get(ws_subscribe_handler))
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

// meta_request - only responsible for processing requests and notifying the service
pub async fn meta_request(
    State(state): State<Arc<MetaServerAppState>>,
    Json(msg_request): Json<SyncRequest>,
) -> Json<DataSyncResponse> {
    info!("Event processing");

    let response = state.data_transfer.send_request(msg_request).await.unwrap();

    // Notify subscribers using the notification service
    let event = NotificationRequest::Data(response.clone());
    state.notification_service.update(event).await;

    Json(response)
}

// WebSocket subscribe handler - only responsible for subscription
async fn ws_subscribe_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<MetaServerAppState>>,
    Json(user_id): Json<UserId>,
) -> impl IntoResponse {
    let ntf_service = state.notification_service.clone();
    ws.on_upgrade(move |socket| handle_subscription(socket, ntf_service, user_id))
}

async fn handle_subscription(
    socket: WebSocket,
    ntf_service: Arc<NotificationServiceInterface>,
    user_id: UserId,
) {
    let (ws_sender, _) = socket.split();

    let event = NotificationRequest::Subscription {
        user_id: user_id.clone(),
        sink: Arc::new(Mutex::new(ws_sender)),
    };
    ntf_service.update(event).await;
}
