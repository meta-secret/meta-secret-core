use crate::notification_service::websocket::WebSocketManager;
use axum::extract::ws::{Message, WebSocket};
use flume::{Receiver, Sender};
use futures_util::stream::SplitSink;
use meta_secret_core::node::api::DataSyncResponse;
use meta_secret_core::node::common::model::user::common::UserId;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use std::sync::Arc;
use tokio::sync::Mutex;

type WsSink = Arc<Mutex<SplitSink<WebSocket, Message>>>;

#[derive(Clone)]
pub enum NotificationRequest {
    Subscription { user_id: UserId, sink: WsSink },
    Data(DataSyncResponse),
}

#[derive(Clone)]
pub struct NotificationService;

pub struct NotificationServiceInterface {
    sender: Sender<NotificationRequest>,
}

impl NotificationServiceInterface {
    pub async fn update(&self, event: NotificationRequest) {
        self.sender.send_async(event).await.unwrap();
    }
}

struct NotificationServiceWorker {
    receiver: Receiver<NotificationRequest>,
    socket_registry: WebSocketManager,
}

impl NotificationServiceWorker {
    pub async fn run(mut self) {
        while let Ok(event) = self.receiver.recv_async().await {
            self.handle(event).await;
        }
    }

    async fn handle(&mut self, event: NotificationRequest) {
        match event {
            NotificationRequest::Subscription { user_id, sink } => {
                self.socket_registry.insert(user_id.clone(), sink);
            }
            NotificationRequest::Data(sync_response) => {
                match sync_response {
                    DataSyncResponse::Write(vault_name) => {
                        self.socket_registry.notify(vault_name).await;
                    }
                    DataSyncResponse::Data(data) => {
                        let maybe_vault_name = data.0.iter().find_map(|evt| match evt {
                            GenericKvLogEvent::Local(_) => None,
                            GenericKvLogEvent::Vault(vault_event) => Some(vault_event.vault_name()),
                            GenericKvLogEvent::Ss(ss_event) => Some(ss_event.vault_name()),
                            GenericKvLogEvent::DbError(_) => None,
                        });

                        if let Some(vault_name) = maybe_vault_name {
                            self.socket_registry.notify(vault_name).await;
                        }
                    }
                    DataSyncResponse::ServerTailResponse(_) => {
                        //skip
                    }
                    DataSyncResponse::Error { .. } => {
                        //skip
                    }
                }
            }
        }
    }
}

impl NotificationService {
    pub async fn run() -> NotificationServiceInterface {
        let (sender, receiver) = flume::unbounded();

        let task = tokio::spawn(async move {
            let worker = NotificationServiceWorker {
                receiver,
                socket_registry: WebSocketManager::default(),
            };
            worker.run().await;
        });

        task.await.unwrap();

        NotificationServiceInterface { sender }
    }
}

mod websocket {
    use crate::notification_service::WsSink;
    use axum::extract::ws::{Message, Utf8Bytes};
    use futures_util::SinkExt;
    use meta_secret_core::node::common::model::user::common::UserId;
    use meta_secret_core::node::common::model::vault::vault::VaultName;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct WebSocketManager {
        /// user_id and all his WebSocket sinks
        pub(crate) user_sockets: HashMap<UserId, Vec<WsSink>>,
    }

    impl WebSocketManager {
        pub async fn notify(&mut self, vault_name: VaultName) {
            let user_ids: Vec<UserId> = self.user_sockets
                .keys()
                .filter(|user_id| user_id.vault_name == vault_name)
                .cloned()
                .collect();
            
            for user_id in user_ids {
                if let Some(sockets) = self.user_sockets.get_mut(&user_id) {
                    let mut valid_sockets = Vec::new();
                    
                    for socket in sockets.drain(..) {
                        let is_socket_alive = {
                            let mut sender = socket.lock().await;
                            sender.send(Message::Text(Utf8Bytes::from("update"))).await.is_ok()
                        };
                        
                        if is_socket_alive {
                            valid_sockets.push(socket);
                        }
                        // Dead sockets are automatically dropped
                    }
                    
                    *sockets = valid_sockets;
                }
            }
        }
    }

    impl WebSocketManager {
        pub fn insert(&mut self, user_id: UserId, sink: WsSink) {
            if let Some(user_sockets) = self.user_sockets.get_mut(&user_id) {
                user_sockets.push(sink);
            } else {
                self.user_sockets.insert(user_id, vec![sink]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
    use meta_secret_core::node::api::{DataEventsResponse, DataSyncResponse};
    use meta_secret_core::node::db::events::generic_log_event::{GenericKvLogEvent, VaultKvLogEvent};
    use meta_secret_core::node::db::events::vault::vault_event::VaultObject;
    use meta_secret_core::node::common::model::user::common::UserDataMember;
    use anyhow::Result;
    use meta_secret_core::node::common::model::vault::vault::VaultName;

    #[tokio::test]
    async fn test_notification_service_interface_sending() -> Result<()> {
        let (sender, receiver) = flume::unbounded();
        let notification_service = NotificationServiceInterface { sender };

        let data_request = NotificationRequest::Data(DataSyncResponse::Write(VaultName::test()));
        notification_service.update(data_request).await;

        let received_event = receiver.recv_async().await?;
        match received_event {
            NotificationRequest::Data(DataSyncResponse::Write(_)) => {}
            _ => panic!("Expected write data response"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_notification_worker_handles_responses() -> Result<()> {
        let fixture = FixtureRegistry::base().await?;
        let vault_name = fixture.state.empty.user_creds.client.user().vault_name();
        let user_data = fixture.state.empty.user_creds.client.user();
        let user_member = UserDataMember { user_data };
        
        let (sender, receiver) = flume::unbounded();
        let mut worker = NotificationServiceWorker {
            receiver,
            socket_registry: WebSocketManager::default(),
        };

        // Test Write response
        let write_request = NotificationRequest::Data(DataSyncResponse::Write(vault_name.clone()));
        sender.send_async(write_request).await?;

        // Test Data response with vault events
        let vault_object = VaultObject::sign_up(vault_name.clone(), user_member);
        let generic_event = GenericKvLogEvent::Vault(VaultKvLogEvent::Vault(vault_object));
        let data_events = DataEventsResponse(vec![generic_event]);
        let data_request = NotificationRequest::Data(DataSyncResponse::Data(data_events));
        sender.send_async(data_request).await?;

        // Test Error response (should be skipped)
        let error_request = NotificationRequest::Data(DataSyncResponse::Error { msg: "test".to_string() });
        sender.send_async(error_request).await?;

        // Test ServerTailResponse (should be skipped)
        let server_tail_response = meta_secret_core::node::api::ServerTailResponse {
            device_log_tail: None,
            ss_device_log_tail: None,
        };
        let tail_request = NotificationRequest::Data(DataSyncResponse::ServerTailResponse(server_tail_response));
        sender.send_async(tail_request).await?;

        // Process all events
        for _ in 0..4 {
            if let Ok(event) = worker.receiver.recv_async().await {
                worker.handle(event).await;
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_websocket_manager_empty_notify() -> Result<()> {
        let fixture = FixtureRegistry::base().await?;
        let vault_name = fixture.state.empty.user_creds.client.user().vault_name();
        
        let mut ws_manager = WebSocketManager::default();
        ws_manager.notify(vault_name).await;
        
        Ok(())
    }
}
