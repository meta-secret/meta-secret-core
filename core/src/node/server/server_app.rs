use std::rc::Rc;
use std::time::Duration;

use crate::node::common::data_transfer::MpscReceiver;

use crate::node::logger::MetaLogger;
use crate::node::server::data_sync::{DataSync, DataSyncApi, DataSyncMessage};

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
                        self.logger
                            .debug(format!("Received sync request: {:?}", request).as_str());

                        let new_events_result = self.data_sync.replication(request).await;
                        let new_events = match new_events_result {
                            Ok(data) => {
                                self.logger
                                    .debug(format!("New events for a client: {:?}", data).as_str());
                                data
                            }
                            Err(_) => {
                                self.logger.error("Server. Sync Error");
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