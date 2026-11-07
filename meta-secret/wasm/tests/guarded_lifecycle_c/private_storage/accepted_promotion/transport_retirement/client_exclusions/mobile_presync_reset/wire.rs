use super::*;
#[derive(Clone)]
pub(super) struct Call {
    pub order: usize,
    pub request: SyncRequest,
}
pub(super) struct Reply {
    pub order: usize,
    pub request: SyncRequest,
    pub response: DataSyncResponse,
}
pub(super) struct Forward {
    pub inner: Arc<prior_wire::Forward>,
    pub repo: Arc<observer::Repo>,
    pub calls: Mutex<Vec<Call>>,
    pub replies: Mutex<Vec<Reply>>,
    pub unauthorized: Mutex<Vec<usize>>,
    pub tail_error: AtomicBool,
    pub original: Mutex<Option<Vec<String>>>,
}
impl Forward {
    pub fn new(p: &prior::Peer, r: Arc<observer::Repo>) -> Self {
        Self {
            inner: p.wire.clone(),
            repo: r,
            calls: Mutex::new(vec![]),
            replies: Mutex::new(vec![]),
            unauthorized: Mutex::new(vec![]),
            tail_error: false.into(),
            original: Mutex::new(None),
        }
    }
    pub fn reset(&self) {
        self.calls.lock().unwrap().clear();
        self.replies.lock().unwrap().clear();
        self.unauthorized.lock().unwrap().clear();
        *self.original.lock().unwrap() = None;
        self.repo.unauthorized.store(false, Ordering::SeqCst);
    }
    pub fn same_error<T>(&self, r: &Result<T>) -> bool {
        r.as_ref().err().is_some_and(|e| {
            self.original.lock().unwrap().as_ref()
                == Some(&e.chain().map(ToString::to_string).collect::<Vec<_>>())
        })
    }
    pub fn tails(&self) -> usize {
        self.replies.lock().unwrap().iter().filter(|r|matches!(&r.request,SyncRequest::Read(q) if matches!(&**q,ReadSyncRequest::ServerTail(_)))).count()
    }
    pub fn ss(&self) -> usize {
        self.replies.lock().unwrap().iter().filter(|r|matches!(&r.request,SyncRequest::Read(q) if matches!(&**q,ReadSyncRequest::SsRequest(_)))).count()
    }
}
impl SyncProtocol for Forward {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        self.calls.lock().unwrap().push(Call {
            order: self.repo.tick(),
            request: request.clone(),
        });
        if matches!(&request,SyncRequest::Read(q) if matches!(&**q,ReadSyncRequest::ServerTail(_)))
            && self.tail_error.swap(false, Ordering::SeqCst)
        {
            let e = anyhow::Error::new(TailError).context("actual mobile first ServerTail");
            *self.original.lock().unwrap() = Some(e.chain().map(ToString::to_string).collect());
            return Err(e);
        }
        let r = self.inner.send(request.clone()).await;
        let order = self.repo.tick();
        match &r {
            Ok(response) => self.replies.lock().unwrap().push(Reply {
                order,
                request,
                response: response.clone(),
            }),
            Err(e)
                if e.chain()
                    .any(|x| x.to_string() == "DeviceTransportNoopUnauthorized") =>
            {
                self.unauthorized.lock().unwrap().push(order);
                self.repo.unauthorized.store(true, Ordering::SeqCst);
                *self.original.lock().unwrap() = Some(e.chain().map(ToString::to_string).collect());
            }
            _ => {}
        }
        r
    }
}
