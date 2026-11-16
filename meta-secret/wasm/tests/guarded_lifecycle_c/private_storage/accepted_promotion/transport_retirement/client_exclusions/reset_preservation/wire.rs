use super::*;

#[derive(Clone)]
pub(super) struct WireCall {
    pub order: usize,
    pub request: SyncRequest,
}
pub(super) struct Reply {
    pub order: usize,
    pub response: DataSyncResponse,
}
pub(super) struct Forward {
    pub inner: Arc<prior_wire::Forward>,
    pub repo: Arc<observer::Repo>,
    pub calls: Mutex<Vec<WireCall>>,
    pub replies: Mutex<Vec<Reply>>,
    pub unauthorized: Mutex<Vec<usize>>,
    pub tail_error: AtomicBool,
    pub tail_latch: Latch,
    pub error_chain: Mutex<Option<Vec<String>>>,
}
impl Forward {
    pub fn new(peer: &prior::Peer, repo: Arc<observer::Repo>) -> Self {
        Self {
            inner: peer.wire.clone(),
            repo,
            calls: Mutex::new(vec![]),
            replies: Mutex::new(vec![]),
            unauthorized: Mutex::new(vec![]),
            tail_error: false.into(),
            tail_latch: Latch::new(),
            error_chain: Mutex::new(None),
        }
    }
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.replies.lock().unwrap().clear();
        self.unauthorized.lock().unwrap().clear();
    }
    pub fn original<T>(&self, r: &Result<T>) -> bool {
        r.as_ref().err().is_some_and(|e| {
            e.is::<TailError>()
                && self.error_chain.lock().unwrap().as_ref()
                    == Some(&e.chain().map(ToString::to_string).collect::<Vec<_>>())
        })
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.calls.lock().unwrap().push(WireCall {
            order: self.repo.tick(),
            request: request.clone(),
        });
        if matches!(&request, SyncRequest::Read(r) if matches!(&**r, ReadSyncRequest::ServerTail(_)))
        {
            self.tail_latch.wait().await;
            if self.tail_error.swap(false, Ordering::SeqCst) {
                let error =
                    anyhow::Error::new(TailError).context("actual original ServerTail call");
                *self.error_chain.lock().unwrap() =
                    Some(error.chain().map(ToString::to_string).collect());
                return Err(error);
            }
        }
        let result = self.inner.send(request).await;
        let order = self.repo.tick();
        match &result {
            Ok(r) => self.replies.lock().unwrap().push(Reply {
                order,
                response: r.clone(),
            }),
            Err(e)
                if e.chain()
                    .any(|v| v.to_string() == "DeviceTransportNoopUnauthorized") =>
            {
                self.unauthorized.lock().unwrap().push(order);
            }
            _ => {}
        }
        result
    }
}
