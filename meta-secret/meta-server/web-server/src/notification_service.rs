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
            self.handle(event);
        }
    }

    fn handle(&mut self, event: NotificationRequest) {
        match event {
            NotificationRequest::Subscription { user_id, sink } => {
                self.socket_registry.insert(user_id.clone(), sink);
            }
            NotificationRequest::Data(sync_response) => {
                match sync_response {
                    DataSyncResponse::Empty => {
                        todo!("need user id/ user creds");
                    }
                    DataSyncResponse::Data(data) => {
                        let maybe_vault_name = data.0.iter().find_map(|evt| match evt {
                            GenericKvLogEvent::Local(_) => None,
                            GenericKvLogEvent::Vault(vault_event) => Some(vault_event.vault_name()),
                            GenericKvLogEvent::Ss(ss_event) => Some(ss_event.vault_name()),
                            GenericKvLogEvent::DbError(_) => None,
                        });

                        if let Some(vault_name) = maybe_vault_name {
                            self.socket_registry.notify(vault_name);
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

    use meta_secret_core::node::common::model::user::common::UserId;
    use meta_secret_core::node::common::model::vault::vault::VaultName;
    use std::collections::HashMap;

    pub struct WebSocketManager {
        /// user_id and all his WebSocket sinks
        pub(crate) user_sockets: HashMap<UserId, Vec<WsSink>>,
    }

    impl WebSocketManager {
        pub fn notify(&mut self, vault_name: VaultName) {
            for (user_id, mut sockets) in self.user_sockets {
                if user_id.vault_name != vault_name {
                    continue;
                }
                
                sockets.retain(async |subscriber| {
                    let mut sender = subscriber.lock().await;
                    let res = sender.send(Message::Text(Utf8Bytes::from("update"))).await;
                    match res {
                        Ok(_) => true,
                        Err(_) => false,
                    }
                });
            }
        }
    }

    impl Default for WebSocketManager {
        fn default() -> Self {
            WebSocketManager {
                user_sockets: HashMap::new(),
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

    #[tokio::test]
    async fn test_notification_service_interface_sending() -> anyhow::Result<()> {
        // Create notification service interface
        let (sender, receiver) = flume::unbounded();
        let notification_service = NotificationServiceInterface { sender };

        // Send empty data response
        let data_request = NotificationRequest::Data(DataSyncResponse::Empty);
        notification_service.update(data_request).await;

        // Verify message was sent through the channel
        let received_event = receiver.recv_async().await?;
        match received_event {
            NotificationRequest::Data(DataSyncResponse::Empty) => {
                // Expected behavior - message was transmitted correctly
            }
            _ => panic!("Expected empty data response"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_notification_worker_handles_data_empty() -> anyhow::Result<()> {
        let (sender, receiver) = flume::unbounded();
        let mut worker = NotificationServiceWorker {
            receiver,
            socket_registry: WebSocketManager::default(),
        };

        // Send empty data response
        let data_request = NotificationRequest::Data(DataSyncResponse::Empty);
        sender.send_async(data_request).await?;

        // Process one event - this should hit the todo!() for empty responses
        if let Ok(event) = worker.receiver.recv_async().await {
            // The current implementation has a todo!() for empty responses
            // In a real implementation, this would be handled properly
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker.handle(event);
            }));
            
            // Expect the panic due to the todo!() in the current implementation
            assert!(result.is_err(), "Expected panic due to todo!() in empty response handling");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_notification_worker_extracts_vault_name() -> anyhow::Result<()> {
        let fixture = FixtureRegistry::base().await?;
        let vault_name = fixture.state.empty.user_creds.client.user().vault_name();
        let user_data = fixture.state.empty.user_creds.client.user();
        let user_member = UserDataMember { user_data };
        
        let (sender, receiver) = flume::unbounded();
        let mut worker = NotificationServiceWorker {
            receiver,
            socket_registry: WebSocketManager::default(),
        };

        // Create a vault event
        let vault_object = VaultObject::sign_up(vault_name.clone(), user_member);
        let generic_event = GenericKvLogEvent::Vault(VaultKvLogEvent::Vault(vault_object));
        
        // Create data response with vault event
        let data_events = DataEventsResponse(vec![generic_event]);
        let data_request = NotificationRequest::Data(DataSyncResponse::Data(data_events));
        
        sender.send_async(data_request).await?;

        // Process one event - this should extract the vault name and call notify
        if let Ok(event) = worker.receiver.recv_async().await {
            worker.handle(event);
        }

        // The worker should have processed the event without errors
        Ok(())
    }

    #[tokio::test]
    async fn test_notification_worker_handles_error_response() -> anyhow::Result<()> {
        let (sender, receiver) = flume::unbounded();
        let mut worker = NotificationServiceWorker {
            receiver,
            socket_registry: WebSocketManager::default(),
        };

        // Send error response
        let error_msg = "Test error message".to_string();
        let error_request = NotificationRequest::Data(DataSyncResponse::Error { msg: error_msg });
        sender.send_async(error_request).await?;

        // Process one event
        if let Ok(event) = worker.receiver.recv_async().await {
            worker.handle(event);
        }

        // Should complete without errors - error responses are skipped
        Ok(())
    }

    #[tokio::test]
    async fn test_websocket_manager_default() -> anyhow::Result<()> {
        let ws_manager = WebSocketManager::default();
        
        // Verify empty state
        assert!(ws_manager.user_sockets.is_empty());
        
        Ok(())
    }

    #[tokio::test]
    async fn test_websocket_manager_empty_notify() -> anyhow::Result<()> {
        let fixture = FixtureRegistry::base().await?;
        let vault_name = fixture.state.empty.user_creds.client.user().vault_name();
        
        let mut ws_manager = WebSocketManager::default();
        
        // Notify on empty manager - should not panic
        ws_manager.notify(vault_name);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_data_sync_response_variants() -> anyhow::Result<()> {
        // Test different DataSyncResponse variants to ensure they compile and work
        let empty_response = DataSyncResponse::Empty;
        let error_response = DataSyncResponse::Error { msg: "test".to_string() };
        
        // Test that we can create NotificationRequest variants
        let data_request_empty = NotificationRequest::Data(empty_response);
        let data_request_error = NotificationRequest::Data(error_response);
        
        // Verify they match correctly
        match data_request_empty {
            NotificationRequest::Data(DataSyncResponse::Empty) => {
                // Expected
            }
            _ => panic!("Expected empty data request"),
        }
        
        match data_request_error {
            NotificationRequest::Data(DataSyncResponse::Error { msg }) => {
                assert_eq!(msg, "test");
            }
            _ => panic!("Expected error data request"),
        }
        
        Ok(())
    }

    #[tokio::test]
    async fn test_vault_event_name_extraction() -> anyhow::Result<()> {
        let fixture = FixtureRegistry::base().await?;
        let vault_name = fixture.state.empty.user_creds.client.user().vault_name();
        let user_data = fixture.state.empty.user_creds.client.user();
        let user_member = UserDataMember { user_data };
        
        // Create a vault event
        let vault_object = VaultObject::sign_up(vault_name.clone(), user_member);
        let generic_event = GenericKvLogEvent::Vault(VaultKvLogEvent::Vault(vault_object));
        
        // Test vault name extraction
        match &generic_event {
            GenericKvLogEvent::Vault(vault_event) => {
                let extracted_vault_name = vault_event.vault_name();
                assert_eq!(extracted_vault_name, vault_name);
            }
            _ => panic!("Expected vault event"),
        }
        
        Ok(())
    }

    #[tokio::test]
    async fn test_notification_worker_skips_non_vault_events() -> anyhow::Result<()> {
        let (sender, receiver) = flume::unbounded();
        let mut worker = NotificationServiceWorker {
            receiver,
            socket_registry: WebSocketManager::default(),
        };

        // Send ServerTailResponse (should be skipped)
        let server_tail_response = meta_secret_core::node::api::ServerTailResponse {
            device_log_tail: None,
            ss_device_log_tail: None,
        };
        let data_request = NotificationRequest::Data(
            DataSyncResponse::ServerTailResponse(server_tail_response)
        );
        
        sender.send_async(data_request).await?;
        
        // Process one event
        if let Ok(event) = worker.receiver.recv_async().await {
            worker.handle(event);
        }
        
        // Should complete without errors - ServerTailResponse events are skipped
        
        Ok(())
    }
}
