use std::sync::Arc;
use axum::extract::{State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::Mutex;
use tracing::error;
use axum::Json;
use meta_secret_core::node::common::model::user::common::UserId;
use crate::MetaServerAppState;
use crate::notification_service::{NotificationRequest, NotificationServiceInterface};

// WebSocket echo handler
pub async fn ws_echo_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_echo_socket)
}

async fn handle_echo_socket(socket: WebSocket) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                let mut sender = ws_sender.lock().await;
                let _ = sender.send(Message::Text(text)).await;
            }
            Message::Ping(p) => {
                let mut sender = ws_sender.lock().await;
                let _ = sender.send(Message::Pong(p)).await;
            }
            Message::Close(_) => {
                break;
            }
            _ => {
                error!("Unsupported WebSocket message type received");
            }
        }
    }
}

// WebSocket subscribe handler - only responsible for subscription
pub async fn ws_subscribe_handler(
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
