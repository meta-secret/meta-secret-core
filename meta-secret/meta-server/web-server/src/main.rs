use axum::extract::State;
use axum::{Json, Router, routing::post};
use http::{StatusCode, Uri};
use serde_derive::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

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
use axum::extract::ws::{WebSocketUpgrade, WebSocket, Message};
use axum::response::IntoResponse;
use futures_util::{StreamExt, SinkExt};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct MetaServerAppState {
    data_transfer: Arc<MetaServerDataTransfer>,
    notifier: broadcast::Sender<String>,
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

    let (notifier, _) = broadcast::channel::<String>(100);
    let app_state = Arc::new(MetaServerAppState { data_transfer, notifier });

    info!("Creating router...");
    let app = Router::new()
        .route("/meta_request", post(meta_request))
        .route("/hi", get(hi))
        .route("/ws", get(ws_handler))
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

    // Notify all websocket subscribers about the state change
    let _ = state.notifier.send("state_changed".to_string());

    Json(response)
}

// WebSocket echo handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<MetaServerAppState>>,
) -> impl IntoResponse {
    let notifier = state.notifier.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, notifier))
}

async fn handle_socket(socket: WebSocket, notifier: broadcast::Sender<String>) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    let mut rx = notifier.subscribe();
    let ws_sender_clone = ws_sender.clone();
    // Spawn a task to forward broadcast messages to the websocket
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let mut sender = ws_sender_clone.lock().await;
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });
    // Echo incoming messages as before
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Ping(p) => {
                let mut sender = ws_sender.lock().await;
                let _ = sender.send(Message::Pong(p)).await;
            }
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }
    // If the client disconnects, stop the send task
    send_task.abort();
}
