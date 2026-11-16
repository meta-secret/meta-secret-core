//! Unconfirmed completion preserves legacy value but cannot issue continuity proof.
use super::{OWNER, harness::Harness, seed_fixture};
use anyhow::{Context, Result, ensure};
use meta_secret_core::node::{
    common::model::secret::SsDistributionStatus,
    db::objects::{
        persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
    },
};
use std::sync::{Arc, atomic::Ordering};

#[test]
fn decoded_completion_error_preserves_legacy_json_success_with_no_receipt() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new_recovery()?;
    h.http.completion_error.store(true, Ordering::SeqCst);
    let claim = h.recovery.as_ref().context("actual accepted claim")?;
    let response: serde_json::Value =
        serde_json::from_str(&crate::json_api::show_recovered(claim.id.wasm_id_str()))?;
    ensure!(
        response["success"] == true
            && response["message"]["secret"].as_str() == Some(seed_fixture::VALUE),
        "legacy decoded-error JSON value/success remain intact"
    );
    ensure!(
        response["message"].get("revealReceipt").is_none(),
        "unconfirmed completion cannot issue usable receipt"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let server = runtime.block_on(
        PersistentSharedSecret::from(Arc::new(PersistentObject::new(h.server_repo.clone())))
            .get_ss_log_obj(h.actor.vault_name.clone()),
    )?;
    ensure!(
        server.claims[&claim.id]
            .status
            .statuses
            .get(h.actor.device_id())
            != Some(&SsDistributionStatus::Delivered),
        "test-owned decoded Error did not perform signed server completion"
    );
    drop(h);
    ensure!(
        crate::mobile_manager::MobileApplicationManager::get_global_instance().is_none(),
        "singleton restored"
    );
    Ok(())
}
