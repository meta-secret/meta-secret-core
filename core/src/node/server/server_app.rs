use std::rc::Rc;
use std::time::Duration;

use flume::{Receiver, RecvError, Sender};

use crate::node::db::models::GenericKvLogEvent;
use crate::node::server::data_sync::{DataSync, DataSyncApi, DataSyncMessage, MetaLogger};

pub struct ServerApp {
    pub timeout: Duration,
    pub data_sync: DataSync,
    pub data_transfer: Rc<MpscReceiver>,
    pub logger: Rc<dyn MetaLogger>,
}

impl ServerApp {
    pub async fn run(&self) {
        loop {
            async_std::task::sleep(self.timeout).await;

            while let Ok(sync_message) = self.data_transfer.receive().await {
                match sync_message {
                    DataSyncMessage::SyncRequest(request) => {
                        let new_events_result = self.data_sync.replication(request).await;
                        let new_events = match new_events_result {
                            Ok(data) => data,
                            Err(_) => {
                                vec![]
                            }
                        };

                        self.data_transfer.reply(new_events).await;
                    }
                    DataSyncMessage::Event(event) => {
                        self.data_sync.send(&event).await;
                    }
                }
            }
        }
    }
}

pub struct MpscDataTransfer {
    pub mpsc_sender: Rc<MpscSender>,
    pub mpsc_receiver: Rc<MpscReceiver>,
}

pub struct MpscSender {
    sender: Sender<DataSyncMessage>,
    receiver: Receiver<Vec<GenericKvLogEvent>>,
}

pub struct MpscReceiver {
    callback: Sender<Vec<GenericKvLogEvent>>,
    receiver: Receiver<DataSyncMessage>,
}

impl MpscDataTransfer {
    pub fn new() -> MpscDataTransfer {
        let (client_sender, client_receiver) = flume::unbounded();
        let (server_sender, server_receiver) = flume::unbounded();

        MpscDataTransfer {
            mpsc_sender: Rc::new(MpscSender {
                sender: client_sender,
                receiver: server_receiver,
            }),
            mpsc_receiver: Rc::new(MpscReceiver {
                callback: server_sender,
                receiver: client_receiver,
            }),
        }
    }
}

impl MpscSender {
    pub async fn send(&self, message: DataSyncMessage) {
        let _ = self.sender.send_async(message).await;
    }

    pub async fn on_update(&self) -> Result<Vec<GenericKvLogEvent>, RecvError> {
        self.receiver.recv_async().await
    }
}

impl MpscReceiver {
    async fn receive(&self) -> Result<DataSyncMessage, RecvError> {
        self.receiver.recv_async().await
    }

    pub async fn reply(&self, events: Vec<GenericKvLogEvent>) {
        let _ = self.callback.send_async(events).await;
    }
}
