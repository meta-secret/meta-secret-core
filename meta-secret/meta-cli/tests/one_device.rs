use axum::{Json, Router, extract::State, routing::post};
use meta_secret_core::crypto::key_pair::MasterKeyManager;
use meta_secret_core::node::api::{DataSyncResponse, SyncRequest};
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_server_node::server::server_app::{MetaServerDataTransfer, ServerApp};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

struct LocalServer {
    endpoint: String,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl LocalServer {
    fn start() -> Self {
        let (ready_sender, ready_receiver) = mpsc::channel();
        let (stop, shutdown) = oneshot::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            tokio::task::LocalSet::new().block_on(&runtime, async move {
                let key = MasterKeyManager::from_pure_sk(MasterKeyManager::generate_sk());
                let server = Arc::new(
                    ServerApp::new(Arc::new(InMemKvLogEventRepo::default()), key).unwrap(),
                );
                let transfer = server.get_data_transfer();
                tokio::task::spawn_local(async move {
                    let _ = server.run().await;
                });
                let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
                ready_sender
                    .send(format!("http://{}", listener.local_addr().unwrap()))
                    .unwrap();
                let app = Router::new()
                    .route("/meta_request", post(meta_request))
                    .with_state(transfer);
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown.await;
                    })
                    .await
                    .unwrap();
            });
        });
        Self {
            endpoint: ready_receiver.recv().unwrap(),
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn meta_request(
    State(transfer): State<Arc<MetaServerDataTransfer>>,
    Json(request): Json<SyncRequest>,
) -> Json<DataSyncResponse> {
    Json(transfer.send_request(request).await.unwrap())
}

#[test]
fn test_one_device_lifecycle() {
    let server = LocalServer::start();
    let directory = TempDir::new().unwrap();
    let command = |arguments: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_meta-cli"));
        command
            .args(arguments)
            .current_dir(directory.path())
            .env("METASECRET_E2E_SERVER_URL", &server.endpoint);
        command
    };
    let success = |output: Output| {
        assert!(
            output.status.success(),
            "CLI command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    success(
        command(&["init", "device", "--device-name", "local-test"])
            .output()
            .unwrap(),
    );
    success(
        command(&["init", "user", "--vault-name", "local-cli@test.invalid"])
            .output()
            .unwrap(),
    );
    success(command(&["auth", "sign-up"]).output().unwrap());
    let mut child = command(&["secret", "split", "--pass-name", "Example", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"test-only-value")
            .unwrap();
    }
    success(child.wait_with_output().unwrap());
    let output = success(
        command(&[
            "--output-format",
            "json",
            "secret",
            "show-local",
            "--pass-name",
            "Example",
        ])
        .output()
        .unwrap(),
    );
    let shown: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(shown["secret"], "test-only-value");
    let output = success(
        command(&["secret", "delete", "--pass-name", "Example", "--yes"])
            .output()
            .unwrap(),
    );
    let deleted: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(deleted["clientStatus"], "Done");
    assert!(deleted["requestId"].as_str().is_some());
    let output = success(
        command(&["--output-format", "json", "info", "secrets"])
            .output()
            .unwrap(),
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Example"));
    assert!(
        !command(&["secret", "show-local", "--pass-name", "Example"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
