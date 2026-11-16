use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{ApplicationState, WasmApplicationState};
use serde_json::Value;

fn object_keys_allowed(value: &Value, allowed: &[&str]) -> Result<()> {
    assert!(
        value
            .as_object()
            .context("metadata object")?
            .keys()
            .all(|key| allowed.contains(&key.as_str())),
        "Liveboard metadata contains a field outside its public allowlist"
    );
    Ok(())
}

#[tokio::test]
async fn actual_core_member_native_json_and_wasm_getter_share_one_safe_metadata_snapshot()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("binding-metadata-only").await?;
    let removal = h.seed_removal().await?;
    let state = h.service().get_app_state().await?;
    assert!(matches!(&state, ApplicationState::Vault(_)));
    // Exact wrapper used by mobile json_api::get_state, with a real Core state.
    let native_equivalent = serde_json::json!({"success": true, "message": {"state": state}});
    let native = &native_equivalent["message"]["state"]["vault"]["member"]["liveboardState"];
    assert!(
        native.is_object(),
        "RED: actual member JSON has no Liveboard metadata snapshot"
    );
    let wasm = WasmApplicationState::from(state.clone())
        .as_vault()
        .as_member();
    let serialized = wasm
        .liveboard_state_json()
        .map_err(|_| anyhow::anyhow!("Liveboard getter failed"))?;
    let wasm_value: Value = serde_json::from_str(&serialized)?;
    assert_eq!(
        &wasm_value, native,
        "WASM must expose the same stored snapshot without rebuilding/syncing"
    );
    object_keys_allowed(
        native,
        &[
            "schemaVersion",
            "vaultName",
            "currentDeviceId",
            "storageMode",
            "canCreateSecret",
            "removableDeviceIds",
            "secrets",
            "operations",
        ],
    )?;
    assert_eq!(native["schemaVersion"], 1);
    assert_eq!(
        native["currentDeviceId"],
        h.approver.device_id().to_string()
    );
    assert_eq!(native["storageMode"], "shares");
    let secrets = native["secrets"]
        .as_array()
        .context("secret metadata array")?;
    assert!(
        secrets
            .iter()
            .any(|secret| secret["secretId"] == pass.id_str())
    );
    for secret in secrets {
        object_keys_allowed(
            secret,
            &[
                "secretId",
                "primaryAction",
                "recoveryClaimId",
                "deletionRequestId",
                "canDelete",
                "readiness",
            ],
        )?;
    }
    let operations = native["operations"]
        .as_array()
        .context("operation metadata array")?;
    assert!(
        operations
            .iter()
            .any(|operation| operation["kind"] == "removeDevice"
                && operation["requestId"] == REQUEST)
    );
    for operation in operations {
        object_keys_allowed(
            operation,
            &[
                "kind",
                "requestId",
                "requesterId",
                "targetDeviceId",
                "secretId",
                "clientStatus",
                "eligiblePeerIds",
                "canApprove",
                "canDecline",
                "outcome",
                "responderId",
                "resolutionSource",
                "readiness",
            ],
        )?;
    }
    let public = serialized;
    assert!(!public.contains(&serde_json::to_string(&removal.share_transfers)?));
    for marker in [
        VALUE,
        "shareBlocks",
        "secretMessage",
        "cipherText",
        "signature",
        "secretBox",
        "transportSk",
        "masterKey",
        "workflowArtifactId",
        "verifiedEnvelopeHash",
    ] {
        assert!(
            !public.contains(marker),
            "new metadata DTO must exclude secret/packet/key fields"
        );
    }
    // Verify the additive snapshot survives the native serializable state roundtrip.
    let roundtrip: ApplicationState =
        serde_json::from_value(native_equivalent["message"]["state"].clone())?;
    let again = WasmApplicationState::from(roundtrip).as_vault().as_member();
    let again: Value = serde_json::from_str(
        &again
            .liveboard_state_json()
            .map_err(|_| anyhow::anyhow!("roundtrip getter failed"))?,
    )?;
    assert_eq!(again, wasm_value);
    Ok(())
}
