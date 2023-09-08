use flume::{Receiver, RecvError, Sender};
use std::rc::Rc;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::server::data_sync::DataSyncMessage;

pub struct MpscDataTransfer {
    pub mpsc_service: Rc<MpscSender>,
    pub mpsc_client: Rc<MpscReceiver>,
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
        let (server_sender, server_receiver) = flume::unbounded();
        let (client_sender, client_receiver) = flume::unbounded();

        MpscDataTransfer {
            mpsc_service: Rc::new(MpscSender {
                sender: client_sender,
                receiver: server_receiver,
            }),
            mpsc_client: Rc::new(MpscReceiver {
                callback: server_sender,
                receiver: client_receiver,
            }),
        }
    }
}

impl MpscSender {
    pub async fn just_send(&self, message: DataSyncMessage) {
        let _ = self.sender.send_async(message).await;
    }

    pub async fn send_and_get(&self, message: DataSyncMessage) -> Result<Vec<GenericKvLogEvent>, RecvError> {
        let _ = self.sender.send_async(message).await;
        self.receiver.recv_async().await
    }
}

impl MpscReceiver {
    pub async fn receive(&self) -> Result<DataSyncMessage, RecvError> {
        self.receiver.recv_async().await
    }

    pub async fn reply(&self, events: Vec<GenericKvLogEvent>) {
        let _ = self.callback.send_async(events).await;
    }
}
