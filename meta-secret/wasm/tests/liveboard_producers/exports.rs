//! Host type/symbol checks are separate from runtime native JSON behavior.
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    device::common::DeviceId, liveboard_producer::ProducerReference, meta_pass::MetaPasswordId,
};
use meta_secret_wasm::wasm_app_manager::WasmApplicationManager;
use serde_json::{Value, json};

async fn wasm_borrow_types(
    manager: &WasmApplicationManager,
    pass: &MetaPasswordId,
    target: &DeviceId,
) {
    // Compile-only: these futures are NEVER executed on host; no JsValue/JsError
    // construction or host panic is used as semantic RED.
    let _ = manager.delete_secret_checked(pass).await;
    let _ = manager.recover_checked(pass).await;
    let _ = manager.remove_member_checked(target).await;
    let _ = manager.reconcile_producer_receipt(String::new()).await;
    let _ = manager.continue_local_remove_checked(String::new()).await;
    let _ = pass.id_str();
    let _ = target.to_string();
}
async fn native_type_signatures(
    manager: &mobile_common::mobile_manager::MobileApplicationManager,
    input: ProducerReference,
) {
    let _ = manager.delete_secret_checked("opaque-secret-id").await;
    let _ = manager.remove_member_checked("opaque-device-id").await;
    let _ = manager.reconcile_producer_receipt(input.clone()).await;
    let _ = manager.continue_local_remove_checked(input).await;
}
#[test]
fn new_producer_outer_symbols_and_borrowed_rust_signatures_compile_control() {
    let _ = wasm_borrow_types;
    let _ = native_type_signatures;
    let _: fn(String) -> String = metasecret_mobile::delete_secret_checked;
    let _: fn(String) -> String = metasecret_mobile::remove_member_checked;
    let _: fn(String) -> String = metasecret_mobile::reconcile_producer_receipt;
    let _: fn(String) -> String = metasecret_mobile::continue_local_remove_checked;
}
#[test]
fn native_common_and_uniffi_producer_uninitialized_errors_are_exact_and_payload_free() -> Result<()>
{
    assert!(!mobile_common::mobile_manager::MobileApplicationManager::is_global_initialized());
    for (common, ffi) in [
        (
            mobile_common::json_api::delete_secret_checked as fn(String) -> String,
            metasecret_mobile::delete_secret_checked as fn(String) -> String,
        ),
        (
            mobile_common::json_api::remove_member_checked as fn(String) -> String,
            metasecret_mobile::remove_member_checked as fn(String) -> String,
        ),
    ] {
        let first: Value = serde_json::from_str(&common("PRIVATE-OPAQUE-INPUT".into()))?;
        let second: Value = serde_json::from_str(&ffi("PRIVATE-OPAQUE-INPUT".into()))?;
        assert_eq!(first, second);
        assert_eq!(
            first,
            json!({"success":false,"errorCode":"NotInitialized"}),
            "RED: new producer native wrapper must use fixed not-initialized envelope"
        );
    }
    Ok(())
}
#[test]
fn producer_reference_errors_validate_before_common_and_uniffi_manager_lookup() -> Result<()> {
    assert!(!mobile_common::mobile_manager::MobileApplicationManager::is_global_initialized());
    for (common, ffi) in [
        (
            mobile_common::json_api::reconcile_producer_receipt as fn(String) -> String,
            metasecret_mobile::reconcile_producer_receipt as fn(String) -> String,
        ),
        (
            mobile_common::json_api::continue_local_remove_checked as fn(String) -> String,
            metasecret_mobile::continue_local_remove_checked as fn(String) -> String,
        ),
    ] {
        for input in [
            "PRIVATE-INVALID-REFERENCE".to_string(),
            "x".repeat(4097),
            "{\"version\":2}".into(),
        ] {
            let first: Value = serde_json::from_str(&common(input.clone()))?;
            let second: Value = serde_json::from_str(&ffi(input))?;
            assert_eq!(first, second);
            assert_eq!(
                first,
                json!({"success":false,"errorCode":"InvalidProducerReference"}),
                "RED: nested reference must validate before singleton lookup"
            );
        }
    }
    Ok(())
}
#[test]
fn uniffi_additive_producer_udl_strings_preserve_established_receipt_exports_control() -> Result<()>
{
    let udl = include_str!("../../../mobile/uniffi/src/mobile_uniffi.udl");
    for line in [
        "string delete_secret_checked(string secret_id);",
        "string remove_member_checked(string target_device_id);",
        "string reconcile_producer_receipt(string reference_json);",
        "string continue_local_remove_checked(string reference_json);",
        "string recover_checked(string secret_id);",
        "string decide_join_request(string request_json);",
        "string decide_remove_member(string request_json);",
        "string delete_secret(string secret_id);",
        "string remove_member(string target_device_id);",
    ] {
        anyhow::ensure!(
            udl.contains(line),
            "required additive/legacy UDL declaration missing"
        );
    }
    // Source assertion complements compiled UniFFI symbols and runtime JSON
    // tests; it does not claim generated Swift/Kotlin/JavaScript ABI execution.
    udl.lines()
        .find(|line| line.contains("namespace mobile_uniffi"))
        .context("real UDL namespace")?;
    Ok(())
}
