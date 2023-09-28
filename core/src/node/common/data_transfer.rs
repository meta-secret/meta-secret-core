use flume::{Receiver, RecvError, Sender};

pub struct MpscDataTransfer<Request, Response> {
    pub service_channel: MpscServiceChannel<Request>,
    pub client_channel: MpscClientChannel<Response>,
}

pub struct MpscServiceChannel<Request> {
    sender: Sender<Request>,
    receiver: Receiver<Request>,
}

impl<Request> MpscServiceChannel<Request> {
    fn new() -> MpscServiceChannel<Request> {
        let (server_sender, server_receiver) = flume::unbounded();
        MpscServiceChannel {
            sender: server_sender,
            receiver: server_receiver,
        }
    }
}

pub struct MpscClientChannel<Response> {
    sender: Sender<Response>,
    receiver: Receiver<Response>,
}

impl<Response> MpscClientChannel<Response> {
    fn new() -> MpscClientChannel<Response> {
        let (client_sender, client_receiver) = flume::unbounded();
        MpscClientChannel {
            sender: client_sender,
            receiver: client_receiver,
        }
    }
}

impl<Request, Response> MpscDataTransfer<Request, Response> {
    pub fn new() -> MpscDataTransfer<Request, Response> {
        MpscDataTransfer {
            service_channel: MpscServiceChannel::new(),
            client_channel: MpscClientChannel::new(),
        }
    }
}

impl<Request, Response> MpscDataTransfer<Request, Response> {
    pub async fn send_to_service(&self, message: Request) {
        let _ = self.service_channel.sender.send_async(message).await;
    }

    pub async fn service_receive(&self) -> Result<Request, RecvError> {
        self.service_channel.receiver.recv_async().await
    }

    pub async fn send_to_service_and_get(&self, message: Request) -> Result<Response, RecvError> {
        let _ = self.service_channel.sender.send_async(message).await;
        //receive a message from the service via client channel
        self.client_channel.receiver.recv_async().await
    }

    pub async fn send_to_client(&self, events: Response) {
        let _ = self.client_channel.sender.send_async(events).await;
    }
}
