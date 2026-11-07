//! Real guarded mobile/server completion boundary: no runtime hooks or UI claims.
#![cfg(not(target_arch = "wasm32"))]
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_reveal/fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_producers/protocol.rs"]
#[allow(dead_code)]
mod protocol;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    api::*,
    app::{
        meta_app::meta_client_service::MetaClientService,
        sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
    },
    common::model::{ApplicationState, VaultFullInfo},
    db::{
        events::{generic_log_event::*, shared_secret_event::SsWorkflowObject},
        in_mem_db::InMemKvLogEventRepo,
        objects::persistent_object::PersistentObject,
        repo::generic_db::*,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Boundary {
    network: Arc<protocol::Network>,
    local: Arc<PersistentObject<fault_repo::Repo>>,
    replacement: SsWorkflowObject,
    mode: u8,
    entered: AtomicBool,
}
impl SyncProtocol for Boundary {
    async fn send(&self, request: SyncRequest) -> Result<DataSyncResponse> {
        if matches!(&request, SyncRequest::Read(read) if matches!(read.as_ref(), ReadSyncRequest::SsRecoveryCompletion(_)))
        {
            self.entered.store(true, Ordering::SeqCst);
            match self.mode {
                0 => {
                    self.local.repo.replace(self.replacement.clone()).await?;
                }
                1 => return Ok(DataSyncResponse::Empty),
                2 => anyhow::bail!("SyntheticTransportFailure"),
                _ => {}
            }
        }
        self.network.send(request).await
    }
}
async fn run(mode: u8) -> Result<()> {
    let r = fixture::Recovery::new().await?;
    let original = r.source().await?;
    let replacement = r.packet(&r.h.initiator, &r.h.initiator).await?;
    ensure!(
        original.obj_id() == replacement.obj_id() && original != replacement,
        "actual alternate ciphertext at SAME physical key"
    );
    r.h.p.repo.replace(original).await?;
    let repo = Arc::new(InMemKvLogEventRepo::default());
    for value in r.h.p.repo.get_db().await.into_values() {
        if !matches!(
            value,
            GenericKvLogEvent::UserCreds(_) | GenericKvLogEvent::DeviceCreds(_)
        ) {
            repo.save(value).await?;
        }
    }
    let server = Arc::new(meta_server_node::server::server_app::ServerApp::new(
        repo.clone(),
        r.h.master.clone(),
    )?);
    server.init().await?;
    let boundary = Arc::new(Boundary {
        network: Arc::new(protocol::Network::new(server, repo)),
        local: r.h.p.clone(),
        replacement,
        mode,
        entered: AtomicBool::new(false),
    });
    let gateway = Arc::new(SyncGateway::new(
        "capture-boundary".into(),
        r.h.p.clone(),
        boundary.clone(),
        r.h.master.clone(),
    ));
    let original = r.h.service();
    let service = Arc::new(MetaClientService {
        data_transfer: original.data_transfer,
        sync_gateway: gateway.clone(),
        state_provider: original.state_provider,
        p_obj: r.h.p.clone(),
        device_data: r.h.approver.device(),
        master_key: r.h.master.clone(),
    });
    let mobile = mobile_common::app_manager::ApplicationManager::new(
        boundary.clone(),
        gateway,
        service.clone(),
        r.h.master.clone(),
    );
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = service.get_app_state().await?
    else {
        anyhow::bail!("actual initial network Member")
    };
    let before = serde_json::to_value(member)?["liveboardState"].clone();
    let context = before["operations"]
        .as_array()
        .context("actual accepted operations")?
        .iter()
        .find(|op| op["requestId"] == r.claim.id.wasm_id_str())
        .context("actual accepted original Claim")?
        .get("revealContext")
        .cloned()
        .context("actual original accepted context")?;
    let result = tokio::select! {
        result = mobile.show_recovered_checked(r.claim.id.clone()) => result,
        result = service.run() => {result?;anyhow::bail!("service ended before recovery")},
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => anyhow::bail!("bounded recovery watchdog"),
    };
    ensure!(
        boundary.entered.load(Ordering::SeqCst),
        "mutation enters only after actual reconstruction at completion call"
    );
    if mode == 2 {
        ensure!(
            result.is_err(),
            "transport failure retains old failure behavior"
        );
        return Ok(());
    }
    let result = result?;
    ensure!(
        result.plaintext.text == fixtures::VALUE,
        "legacy original plaintext stays exact"
    );
    if mode == 1 {
        ensure!(
            result.reveal_receipt.is_none(),
            "Empty is not persisted completion proof"
        );
        return Ok(());
    }
    let receipt = result
        .reveal_receipt
        .context("actual capture before completion mutation")?;
    ensure!(
        serde_json::to_value(&receipt.reveal_context)? == context,
        "later source cannot retag old plaintext"
    );
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = service.get_app_state().await?
    else {
        anyhow::bail!("fresh actual Member")
    };
    let after = serde_json::to_value(member)?["liveboardState"].clone();
    let done = after["operations"]
        .as_array()
        .context("ops")?
        .iter()
        .find(|op| op["requestId"] == r.claim.id.wasm_id_str())
        .context("actual Done")?;
    ensure!(
        done["clientStatus"] == "done",
        "actual canonical completion"
    );
    if mode == 0 {
        ensure!(
            done.get("revealContext") != Some(&context),
            "current changed ciphertext cannot match old receipt"
        );
    } else {
        ensure!(
            done.get("revealContext") == Some(&context),
            "normal Accepted/captured receipt/current Done context equality"
        );
    }
    let selected =
        |state: &serde_json::Value| -> Result<serde_json::Map<String, serde_json::Value>> {
            let mut secret = state["secrets"]
                .as_array()
                .context("secrets")?
                .iter()
                .find(|s| s["secretId"] == r.pass.id_str())
                .context("exact Secret")?
                .as_object()
                .context("Secret object")?
                .clone();
            secret.remove("primaryAction");
            secret.remove("recoveryClaimId");
            Ok(secret)
        };
    ensure!(
        selected(&before)? == selected(&after)?,
        "all remaining Secret selection fields stay equal across only action/claim completion delta"
    );
    Ok(())
}
#[tokio::test]
async fn captured_receipt_cannot_be_retagged_by_replacement_between_reconstruction_and_completion()
-> Result<()> {
    run(0).await
}
#[tokio::test]
async fn empty_completion_keeps_legacy_value_but_issues_no_receipt() -> Result<()> {
    run(1).await
}
#[tokio::test]
async fn transport_completion_failure_remains_failure() -> Result<()> {
    run(2).await
}
#[tokio::test]
async fn normal_completion_preserves_every_remaining_secret_selection_field() -> Result<()> {
    run(3).await
}
