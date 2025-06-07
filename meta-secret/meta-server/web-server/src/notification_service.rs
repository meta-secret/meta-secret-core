use meta_secret_core::node::api::DataSyncResponse;
use std::sync::Arc;
use axum::extract::ws::{Message, WebSocket};
use flume::{Receiver, Sender};
use futures_util::SinkExt;
use futures_util::stream::SplitSink;
use tokio::sync::Mutex;
use meta_secret_core::node::common::model::user::common::UserId;
use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::notification_service::websocket::WebSocketManager;

type WsSink = Arc<Mutex<SplitSink<WebSocket, Message>>>;

#[derive(Clone)]
pub enum NotificationRequest {
    Subscription {
        user_id: UserId,
        sink: WsSink,
    },
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
    socket_registry: WebSocketManager
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
                        let maybe_user_id = data.0.iter().find_map(|evt| {
                            match evt {
                                GenericKvLogEvent::Local(_) => None,
                                GenericKvLogEvent::Vault(vault_event) => {
                                    Some(vault_event.user_id())
                                }
                                GenericKvLogEvent::Ss(ss_event) => {
                                    Some(ss_event.user_id())
                                }
                                GenericKvLogEvent::DbError(_) => None
                            }
                        });

                        if let Some(user_id) = maybe_user_id {
                            self.socket_registry.notify(user_id);
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

        NotificationServiceInterface {
            sender,
        }
    }
}


mod websocket {
    use std::collections::HashMap;
    use axum::extract::ws::Message;
    use futures_util::SinkExt;
    use meta_secret_core::node::common::model::user::common::UserId;
    use crate::notification_service::WsSink;

    pub struct WebSocketManager {
        /// user_id and all his WebSocket sinks
        user_sockets: HashMap<UserId, Vec<WsSink>>,
    }

    impl WebSocketManager {
        pub fn notify(&mut self, user_id: UserId) {
            let mut sockets = self.user_sockets.get_mut(&user_id);
            
            for mut s in sockets {
                s.retain(async |subscriber| {
                    let mut sender = subscriber.lock().await;
                    let res = sender.send(Message::Text("q")).await;
                    match res {
                        Ok(_) => {
                            true
                        }
                        Err(_) => {
                            false
                        }
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
