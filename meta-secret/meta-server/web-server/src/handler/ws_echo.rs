use std::sync::Arc;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::Mutex;

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
            _ => {}
        }
    }
}

