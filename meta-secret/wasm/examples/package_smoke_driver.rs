//! Test-only real HTTP ServerApp. Control cannot create or approve business state.
#[path = "package_smoke_driver/http.rs"]
mod http;
use anyhow::{Context, Result, ensure};
use meta_secret_core::{
    crypto::key_pair::{KeyPair, TransportDsaKeyPair},
    node::{
        api::{
            DataSyncResponse, ReadSyncRequest, SignedActionPayload, SyncRequest, WriteSyncRequest,
        },
        common::model::liveboard_producer::producer_source_commitment,
        db::{
            events::{
                generic_log_event::ObjIdExtractor,
                vault::vault_log_event::{
                    VaultActionEvent, VaultActionRequestEvent, VaultActionUpdateEvent,
                },
            },
            in_mem_db::InMemKvLogEventRepo,
        },
    },
};
use meta_server_node::server::server_app::ServerApp;
use serde::{Deserialize, Serialize};
use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Business {
    actor_id: String,
    event_id_json: String,
    body_commitment: String,
    kind: &'static str,
    server_ok: bool,
}
#[derive(Clone, Copy)]
enum ReadKind {
    JoinAdmission,
    Vault,
    SharedSecrets,
    RecoveryCompletion,
    ServerTail,
    StateEvents,
    JoinResult,
    Capabilities,
}
fn read_kind(request: &SyncRequest) -> Option<ReadKind> {
    let SyncRequest::Read(read) = request else {
        return None;
    };
    Some(match &**read {
        ReadSyncRequest::JoinAdmission(_) => ReadKind::JoinAdmission,
        ReadSyncRequest::Vault(_) => ReadKind::Vault,
        ReadSyncRequest::SsRequest(_) => ReadKind::SharedSecrets,
        ReadSyncRequest::SsRecoveryCompletion(_) => ReadKind::RecoveryCompletion,
        ReadSyncRequest::ServerTail(_) => ReadKind::ServerTail,
        ReadSyncRequest::StateEventsSubscription(_) => ReadKind::StateEvents,
        ReadSyncRequest::JoinResultSubscription(_) => ReadKind::JoinResult,
        ReadSyncRequest::LiveboardCapabilities(_) => ReadKind::Capabilities,
    })
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadErrors {
    join_admission: u32,
    vault: u32,
    shared_secrets: u32,
    recovery_completion: u32,
    server_tail: u32,
    state_events: u32,
    join_result: u32,
    capabilities: u32,
}
impl ReadErrors {
    fn record(&mut self, kind: ReadKind) {
        *match kind {
            ReadKind::JoinAdmission => &mut self.join_admission,
            ReadKind::Vault => &mut self.vault,
            ReadKind::SharedSecrets => &mut self.shared_secrets,
            ReadKind::RecoveryCompletion => &mut self.recovery_completion,
            ReadKind::ServerTail => &mut self.server_tail,
            ReadKind::StateEvents => &mut self.state_events,
            ReadKind::JoinResult => &mut self.join_result,
            ReadKind::Capabilities => &mut self.capabilities,
        } += 1;
    }
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    writes: u32,
    observations: u32,
    ordinary_reads: u32,
    fault_hits: u32,
    read_errors: ReadErrors,
    business: Vec<Business>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fault {
    enabled: bool,
    actor_id: String,
}
struct State {
    run_id: String,
    origin: String,
    ready: AtomicBool,
    stop: watch::Sender<bool>,
    evidence: Mutex<Evidence>,
    fault: Mutex<Fault>,
}
struct Http {
    method: String,
    path: String,
    origin: Option<String>,
    body: Vec<u8>,
}
struct Call {
    request: SyncRequest,
    business_index: Option<usize>,
    reply: oneshot::Sender<Result<DataSyncResponse>>,
}
struct ContextState {
    shared: Arc<State>,
    bridge: mpsc::Sender<Call>,
}
type Reply = (u16, serde_json::Value);
fn reply(status: u16, value: &impl Serialize) -> Result<Reply> {
    Ok((status, serde_json::to_value(value)?))
}
fn business(request: &SyncRequest) -> Result<Option<Business>> {
    let SyncRequest::Write(write) = request else {
        return Ok(None);
    };
    let WriteSyncRequest::Event(action) = &**write else {
        return Ok(None);
    };
    let event = match &action.payload {
        SignedActionPayload::Event(
            meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent::DeviceLog(e),
        ) => e,
        SignedActionPayload::GuardedRemovalDecision(g) => &g.event,
        _ => return Ok(None),
    };
    let kind = match &event.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(_)) => "deleteRequest",
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(_)) => "removeRequest",
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(_)) => "removeDecision",
        _ => return Ok(None),
    };
    Ok(Some(Business {
        actor_id: action.signer.to_string(),
        event_id_json: serde_json::to_string(&event.obj_id())?,
        body_commitment: producer_source_commitment(event)?,
        kind,
        server_ok: false,
    }))
}

async fn data(http: Http, ctx: &ContextState) -> Result<Reply> {
    let state = &ctx.shared;
    ensure!(
        http.origin.as_ref().is_none_or(|o| o == &state.origin),
        "G4.OriginUnavailable"
    );
    ensure!(http.path == "/meta_request", "G4.RouteUnavailable");
    if http.method == "OPTIONS" {
        return reply(200, &serde_json::json!({}));
    }
    ensure!(http.method == "POST", "G4.MethodUnavailable");
    let request: SyncRequest = serde_json::from_slice(&http.body)?;
    let observation_actor = match &request {
        SyncRequest::Read(r) => match &**r {
            ReadSyncRequest::ServerTail(q) if q.producer_reference.is_some() => {
                Some(q.sender.device.device_id.to_string())
            }
            _ => None,
        },
        _ => None,
    };
    let fail = {
        let f = state.fault.lock().unwrap();
        f.enabled && observation_actor.as_ref() == Some(&f.actor_id)
    };
    let index = {
        let mut e = state.evidence.lock().unwrap();
        if matches!(&request, SyncRequest::Write(_)) {
            e.writes += 1;
        } else if observation_actor.is_some() {
            e.observations += 1;
        } else {
            e.ordinary_reads += 1;
        }
        if fail {
            e.fault_hits += 1;
        }
        if let Some(b) = business(&request)? {
            ensure!(e.business.len() < 256, "G4.ObservationLimit");
            let i = e.business.len();
            e.business.push(b);
            Some(i)
        } else {
            None
        }
    };
    if fail {
        return reply(
            503,
            &DataSyncResponse::Error {
                msg: "OperationUnavailable".into(),
            },
        );
    }
    let (sender, receiver) = oneshot::channel();
    ctx.bridge
        .send(Call {
            request,
            business_index: index,
            reply: sender,
        })
        .await
        .context("G4.ServerUnavailable")?;
    match receiver.await.context("G4.ServerUnavailable")? {
        Ok(value) => reply(200, &value),
        Err(_) => reply(
            500,
            &DataSyncResponse::Error {
                msg: "OperationUnavailable".into(),
            },
        ),
    }
}
fn control(http: Http, state: &State) -> Result<Reply> {
    ensure!(http.origin.is_none(), "G4.ControlHostOnly");
    match (http.method.as_str(), http.path.as_str()) {
        ("GET", "/ready") => reply(
            200,
            &serde_json::json!({"runId":state.run_id,"ready":state.ready.load(Ordering::SeqCst)}),
        ),
        ("GET", "/snapshot") => reply(200, &*state.evidence.lock().unwrap()),
        ("POST", "/fault") => {
            let f: Fault = serde_json::from_slice(&http.body)?;
            ensure!(
                !f.enabled || (!f.actor_id.is_empty() && f.actor_id.len() <= 512),
                "G4.FaultScope"
            );
            *state.fault.lock().unwrap() = f;
            reply(200, &serde_json::json!({"ok":true}))
        }
        ("POST", "/stop") => {
            let _ = state.stop.send(true);
            reply(200, &serde_json::json!({"ok":true}))
        }
        _ => anyhow::bail!("G4.ControlRouteUnavailable"),
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let get = |key: &str| args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone());
    ensure!(
        args.len() == 6
            && get("--data-port").as_deref() == Some("3000")
            && get("--control-port").as_deref() == Some("3001"),
        "G4.Arguments"
    );
    let run_id = get("--run-id").context("G4.RunId")?;
    ensure!(
        !run_id.is_empty()
            && run_id.len() <= 64
            && run_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "G4.RunId"
    );
    let data_listener = TcpListener::bind(("127.0.0.1", 3000))?;
    let control_listener = TcpListener::bind(("127.0.0.1", 3001))?;
    data_listener.set_nonblocking(true)?;
    control_listener.set_nonblocking(true)?;
    let (stop, _) = watch::channel(false);
    let state = Arc::new(State {
        run_id,
        origin: "http://127.0.0.1:3002".into(),
        ready: AtomicBool::new(false),
        stop,
        evidence: Mutex::new(Evidence::default()),
        fault: Mutex::new(Fault::default()),
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (bridge, mut calls) = mpsc::channel::<Call>(16);
    let owner_state = state.clone();
    let worker = std::thread::Builder::new()
        .name("g4-real-server".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || -> Result<()> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let server = ServerApp::new(
                Arc::new(InMemKvLogEventRepo::default()),
                TransportDsaKeyPair::generate().sk(),
            )?;
            runtime.block_on(Box::pin(server.init()))?;
            owner_state.ready.store(true, Ordering::SeqCst);
            // Finish every admitted call even when its HTTP reply receiver was dropped.
            while let Some(call) = calls.blocking_recv() {
                let kind = read_kind(&call.request);
                let actual = runtime.block_on(Box::pin(server.handle_client_request(call.request)));
                if matches!(&actual, Err(_) | Ok(DataSyncResponse::Error { .. }))
                    && let Some(kind) = kind
                {
                    owner_state
                        .evidence
                        .lock()
                        .unwrap()
                        .read_errors
                        .record(kind);
                }
                if let Some(i) = call.business_index {
                    owner_state.evidence.lock().unwrap().business[i].server_ok = actual
                        .as_ref()
                        .is_ok_and(|v| v.clone().ensure_success().is_ok());
                }
                let _ = call.reply.send(actual);
            }
            Ok(())
        })?;
    let deadline = Instant::now() + Duration::from_secs(600);
    let served = runtime.block_on(http::run(
        data_listener,
        control_listener,
        Arc::new(ContextState {
            shared: state,
            bridge,
        }),
    ));
    // http::run has drained all connections and dropped every bridge sender.
    let owned = worker
        .join()
        .map_err(|_| anyhow::anyhow!("G4.WorkerUnavailable"))?;
    owned?;
    served?;
    ensure!(Instant::now() < deadline, "G4.Deadline");
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("G4.DriverUnavailable");
        std::process::exit(1);
    }
}
