use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use meta_db_sqlite::db::sqlite_store::SqlIteRepo;
use meta_secret_core::node::api::DataSyncResponse;
use meta_secret_core::node::app::sync::meta_ws_url::meta_ws_url_from_http_api_base;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::app::sync::sync_protocol::HttpSyncProtocol;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};
use crate::mobile_manager::with_sync_gateway_lock;

struct MetaWsRuntime {
    stop: Arc<AtomicBool>,
    _join: JoinHandle<()>,
    notify_tx: Sender<()>,
    notify_rx: Arc<Mutex<Receiver<()>>>,
}

static META_WS: Mutex<Option<MetaWsRuntime>> = Mutex::new(None);

pub fn meta_ws_start(
    http_api_base: String,
    gateway: Arc<SyncGateway<SqlIteRepo, HttpSyncProtocol>>,
) -> Result<()> {
    let mut guard = META_WS.lock().unwrap();
    if guard.is_some() {
        anyhow::bail!("meta_ws already running");
    }

    let stop = Arc::new(AtomicBool::new(false));
    let (notify_tx, notify_rx) = mpsc::channel::<()>();
    let notify_rx_wrapped = Arc::new(Mutex::new(notify_rx));

    let url = meta_ws_url_from_http_api_base(&http_api_base);
    let stop_thread = stop.clone();
    let notify_tx_thread = notify_tx.clone();
    let gateway_thread = gateway;

    let join = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                warn!(target: "meta_secret_ws", "failed to build ws runtime: {}", e);
                return;
            }
        };
        rt.block_on(run_meta_ws_loop(
            url,
            gateway_thread,
            stop_thread,
            notify_tx_thread,
        ));
    });

    *guard = Some(MetaWsRuntime {
        stop,
        _join: join,
        notify_tx,
        notify_rx: notify_rx_wrapped,
    });

    Ok(())
}

pub fn meta_ws_stop() -> Result<()> {
    let mut guard = META_WS.lock().unwrap();
    let Some(runtime) = guard.take() else {
        return Ok(());
    };
    runtime.stop.store(true, Ordering::SeqCst);
    drop(runtime.notify_tx);
    let _ = runtime._join.join();
    Ok(())
}

pub fn meta_ws_wait_next_event(timeout_ms: u32) -> bool {
    let rx = {
        let guard = META_WS.lock().unwrap();
        guard.as_ref().map(|r| r.notify_rx.clone())
    };
    let Some(rx) = rx else {
        std::thread::sleep(Duration::from_millis(timeout_ms as u64));
        return false;
    };
    let lock = match rx.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match lock.recv_timeout(Duration::from_millis(timeout_ms as u64)) {
        Ok(()) => true,
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

async fn run_meta_ws_loop(
    url: String,
    gateway: Arc<SyncGateway<SqlIteRepo, HttpSyncProtocol>>,
    stop: Arc<AtomicBool>,
    notify_tx: Sender<()>,
) {
    while !stop.load(Ordering::SeqCst) {
        match connect_async(&url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();
                loop {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let next = read.next().await;
                    match next {
                        Some(Ok(Message::Text(t))) => {
                            match serde_json::from_str::<DataSyncResponse>(&t) {
                                Ok(resp) => match with_sync_gateway_lock(
                                    gateway.apply_data_sync_response(resp)
                                ).await {
                                    Ok(()) => {
                                        let _ = notify_tx.send(());
                                    }
                                    Err(e) => {
                                        warn!(target: "meta_secret_ws", "apply push failed: {:?}", e);
                                    }
                                },
                                Err(e) => {
                                    debug!(target: "meta_secret_ws", "skip non-sync JSON: {}", e);
                                }
                            }
                        }
                        Some(Ok(Message::Ping(p))) => {
                            let _ = write.send(Message::Pong(p)).await;
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(e)) => {
                            warn!(target: "meta_secret_ws", "ws read error: {:?}", e);
                            break;
                        }
                        Some(Ok(Message::Binary(_)))
                        | Some(Ok(Message::Pong(_)))
                        | Some(Ok(Message::Frame(_))) => {}
                    }
                }
            }
            Err(e) => {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                warn!(target: "meta_secret_ws", "ws connect failed: {}, retrying", e);
                eprintln!("[meta_secret_ws] ws connect failed: {}, retrying", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
