//! Colocated native singleton owner: real temporary SQLite/HTTP/Core service.
#[path = "../../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "producer_native_tests/harness.rs"]
mod harness;
#[path = "producer_native_tests/http.rs"]
mod http;
#[path = "producer_native_tests/recovered_show.rs"]
mod recovered_show;
#[path = "../../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod seed_fixture;
#[path = "../../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

use anyhow::{Context, Result};
use harness::Harness;
use meta_secret_core::node::{
    common::model::liveboard_producer::*,
    db::events::{generic_log_event::*, vault::vault_log_event::*},
};
use serde_json::{Value, json};
use std::sync::Mutex;
static OWNER: Mutex<()> = Mutex::new(());
#[path = "producer_native_tests/recovered_controls.rs"]
mod recovered_controls;

fn error(json_text: String, code: &str) -> Result<()> {
    let value: Value = serde_json::from_str(&json_text)?;
    assert_eq!(value, json!({"success":false,"errorCode":code}));
    Ok(())
}
fn decode_receipt(json_text: &str) -> Result<LiveboardProducerReceipt> {
    let value: Value = serde_json::from_str(json_text)?;
    assert_eq!(
        value["success"], true,
        "RED: initialized native checked producer must return actual typed receipt"
    );
    assert_eq!(value.as_object().context("native envelope")?.len(), 2);
    Ok(serde_json::from_value(value["message"].clone())?)
}
fn reference(receipt: &LiveboardProducerReceipt) -> ProducerReference {
    ProducerReference {
        version: receipt.version,
        kind: receipt.kind.clone(),
        request_id: receipt.request_id.clone(),
        request_stage: receipt.request_stage.source.clone(),
        decision_stage: None,
    }
}
#[test]
fn uninitialized_native_producer_calls_use_fixed_minimal_envelope() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert!(super::super::GLOBAL_APP_MANAGER.lock().unwrap().is_none());
    error(
        crate::json_api::delete_secret_checked("PRIVATE-INPUT-MARKER".into()),
        "NotInitialized",
    )?;
    error(
        crate::json_api::remove_member_checked("PRIVATE-DEVICE-MARKER".into()),
        "NotInitialized",
    )
}
#[test]
fn native_invalid_reference_is_rejected_before_singleton_lookup_without_echo() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert!(super::super::GLOBAL_APP_MANAGER.lock().unwrap().is_none());
    for input in [
        "PRIVATE-INVALID-REFERENCE".to_string(),
        "x".repeat(4097),
        "{\"version\":2}".into(),
    ] {
        error(
            crate::json_api::reconcile_producer_receipt(input.clone()),
            "InvalidProducerReference",
        )?;
        error(
            crate::json_api::continue_local_remove_checked(input),
            "InvalidProducerReference",
        )?;
    }
    Ok(())
}
#[test]
fn actual_initialized_native_json_delete_receipt_binds_saved_uuid_source_and_read_only_reconcile()
-> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new()?;
    h.healthy_state()?;
    let result = crate::json_api::delete_secret_checked(h.secret.id_str());
    let receipt = decode_receipt(&result)?;
    assert_eq!(receipt.kind, ProducerKind::DeleteSecret);
    assert_eq!(
        receipt.secret_id.as_deref(),
        Some(h.secret.id_str().as_str())
    );
    let sources = h.sources()?;
    let actual = sources
        .iter()
        .find(|s| {
            matches!(
                s.0.value,
                VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(_))
            )
        })
        .context("actual initialized native saved Delete R")?;
    assert_eq!(receipt.request_stage.source.event_id, actual.obj_id());
    let request = match &actual.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r)) => r,
        _ => unreachable!(),
    };
    assert_eq!(receipt.request_id.as_ref(), Some(&request.request_id));
    let canonical = String::from_utf8(meta_secret_core::node::security::canonical_json(actual)?)?;
    assert!(
        receipt.request_stage.source.body_commitment
            == meta_secret_core::crypto::utils::sha256_hex(&format!(
                "MetaSecret.LiveboardProducer.Source.v1\n{canonical}"
            ))
    );
    let before = h.http.counts.snapshot();
    let source = reference(&receipt);
    let observed = decode_receipt(&crate::json_api::reconcile_producer_receipt(
        serde_json::to_string(&source)?,
    ))?;
    assert_eq!(observed.request_stage.source, source.request_stage);
    assert_eq!(observed.request_id, source.request_id);
    let after = h.http.counts.snapshot();
    assert_eq!(after, (before.0, before.1 + 1, before.2));
    assert!(
        h.sources()? == sources,
        "native reconciliation cannot append R/U or create another ID"
    );
    Ok(())
}
#[test]
fn actual_initialized_native_json_remove_returns_real_target_source_and_occurrence() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new()?;
    h.healthy_state()?;
    let result = crate::json_api::remove_member_checked(h.target.device_id().to_string());
    let receipt = decode_receipt(&result)?;
    assert_eq!(receipt.kind, ProducerKind::RemoveDevice);
    assert_eq!(
        receipt.target_device_id.as_deref(),
        Some(h.target.device_id().to_string().as_str())
    );
    let sources = h.sources()?;
    let actual = sources
        .iter()
        .find(|s| {
            matches!(
                s.0.value,
                VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(_))
            )
        })
        .context("actual native Remove R")?;
    assert_eq!(receipt.request_stage.source.event_id, actual.obj_id());
    assert!(receipt.request_id.is_some());
    assert!(receipt.decision_stage.is_none() && receipt.material_stage.is_none());
    Ok(())
}
#[test]
fn initialized_native_singleton_http_core_state_control() -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let h = Harness::new()?;
    h.healthy_state()
}

#[path = "producer_native_tests/recovery_attempt.rs"]
mod recovery_attempt;

#[path = "recovery_approve_attempt_tests.rs"]
mod recovery_approve_attempt_tests;

#[path = "producer_native_tests/fourth_registration.rs"]
mod fourth_registration;
