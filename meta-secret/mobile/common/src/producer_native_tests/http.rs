//! Local HTTP framing only. Every body goes to the actual signed ServerApp.
use anyhow::{Context, Result, bail};
use meta_secret_core::node::{
    api::{DataSyncResponse, ReadSyncRequest, SyncRequest},
    db::in_mem_db::InMemKvLogEventRepo,
};
use meta_server_node::server::server_app::ServerApp;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
};

#[derive(Default)]
pub struct Counts {
    writes: AtomicUsize,
    observations: AtomicUsize,
    ordinary: AtomicUsize,
}
impl Counts {
    pub fn snapshot(&self) -> (usize, usize, usize) {
        (
            self.writes.load(Ordering::SeqCst),
            self.observations.load(Ordering::SeqCst),
            self.ordinary.load(Ordering::SeqCst),
        )
    }
}
pub struct Http {
    pub port: u16,
    pub counts: Arc<Counts>,
    pub completion_error: Arc<AtomicBool>,
    pub transport_outage: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}
impl Http {
    pub fn start(server: Arc<ServerApp<InMemKvLogEventRepo>>) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let end = stop.clone();
        let counts = Arc::new(Counts::default());
        let observed = counts.clone();
        let completion_error = Arc::new(AtomicBool::new(false));
        let reject_completion = completion_error.clone();
        let transport_outage = Arc::new(AtomicBool::new(false));
        let reject_transport = transport_outage.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            loop {
                let (mut socket, _) = listener.accept()?;
                if end.load(Ordering::SeqCst) {
                    break;
                }
                let request = read(&mut socket)?;
                match &request {
                    SyncRequest::Write(_) => {
                        observed.writes.fetch_add(1, Ordering::SeqCst);
                    }
                    SyncRequest::Read(request) => match request.as_ref() {
                        ReadSyncRequest::ServerTail(input)
                            if input.producer_reference.is_some() =>
                        {
                            observed.observations.fetch_add(1, Ordering::SeqCst);
                        }
                        _ => {
                            observed.ordinary.fetch_add(1, Ordering::SeqCst);
                        }
                    },
                }
                let response = if reject_transport.load(Ordering::SeqCst) {
                    Err(anyhow::anyhow!("controlled HTTP outage"))
                } else if reject_completion.load(Ordering::SeqCst)
                    && matches!(&request, SyncRequest::Read(read) if matches!(read.as_ref(), ReadSyncRequest::SsRecoveryCompletion(_)))
                {
                    Ok(DataSyncResponse::Error {
                        msg: "TestUnconfirmedCompletion".into(),
                    })
                } else {
                    runtime.block_on(Box::pin(server.handle_client_request(request)))
                };
                let (status, body) = match response {
                    Ok(actual) => ("200 OK", serde_json::to_vec(&actual)?),
                    Err(_) => (
                        "500 Internal Server Error",
                        serde_json::to_vec(&DataSyncResponse::Error {
                            msg: "OperationUnavailable".into(),
                        })?,
                    ),
                };
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )?;
                socket.write_all(&body)?;
                socket.flush()?;
            }
            Ok(())
        });
        Ok(Self {
            port,
            counts,
            completion_error,
            transport_outage,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            assert!(
                thread.join().is_ok(),
                "owned real HTTP test thread must stop"
            );
        }
    }
}
fn read(socket: &mut TcpStream) -> Result<SyncRequest> {
    socket.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        let got = socket.read(&mut buffer)?;
        if got == 0 {
            bail!("HTTP fixture closed before headers");
        }
        bytes.extend_from_slice(&buffer[..got]);
        if bytes.len() > 1024 * 1024 {
            bail!("bounded HTTP test input exceeded");
        }
        if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])?;
    let len = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim())
        })
        .context("actual JSON HTTP body length")?
        .parse::<usize>()?;
    if len > 1024 * 1024 {
        bail!("bounded HTTP test body exceeded");
    }
    while bytes.len() < header_end + len {
        let got = socket.read(&mut buffer)?;
        if got == 0 {
            bail!("HTTP fixture body truncated");
        }
        bytes.extend_from_slice(&buffer[..got]);
    }
    Ok(serde_json::from_slice(
        &bytes[header_end..header_end + len],
    )?)
}
