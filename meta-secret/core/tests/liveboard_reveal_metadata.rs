//! Strict optional output metadata parsing. These values never authorize Show.
use meta_secret_core::node::common::model::liveboard_reveal::{
    LiveboardRevealContext, LiveboardRevealReceipt,
};
use serde_json::{Value, json};

fn source(kind: &str, instance: &str) -> Value {
    json!({"eventId":{"fqdn":{"objType":kind,"objInstance":instance},"id":{"curr":1,"prev":0}},"bodyCommitment":"a".repeat(64)})
}
fn context() -> Value {
    json!({"version":1,"vaultName":"synthetic-vault","currentDeviceId":"synthetic-requester",
        "vaultSource":source("Vault","synthetic-vault"),
        "localDistributionSource":source("SsDistribution","synthetic-local"),
        "recoverySources":[source("SsRecovery","synthetic-remote")],"claimScopeCommitment":"b".repeat(64)})
}
fn rejected(value: &Value) -> bool {
    serde_json::from_str::<LiveboardRevealContext>(&value.to_string()).is_err()
}

#[test]
fn malformed_or_unbounded_context_never_deserializes_as_checked_metadata() {
    assert!(serde_json::from_value::<LiveboardRevealContext>(context()).is_ok());
    let cases = [
        ("/version", json!(2)),
        ("/vaultName", json!("")),
        ("/currentDeviceId", json!("x".repeat(1025))),
        ("/claimScopeCommitment", json!("A".repeat(64))),
        ("/vaultSource/eventId/id/curr", json!(0)),
        ("/vaultSource/eventId/id/prev", json!(7)),
        ("/vaultSource/eventId/fqdn/objType", json!("DeviceLog")),
        ("/vaultSource/eventId/unknown", json!(true)),
        ("/vaultSource/eventId/id/unknown", json!(true)),
        ("/vaultSource/eventId/fqdn/unknown", json!(true)),
        ("/recoverySources", json!([])),
        (
            "/recoverySources",
            json!([
                source("SsRecovery", "duplicate"),
                source("SsRecovery", "duplicate")
            ]),
        ),
        (
            "/recoverySources",
            json!([source("SsRecovery", "z"), source("SsRecovery", "a")]),
        ),
        ("/recoverySources", Value::Null),
    ];
    for (path, replacement) in cases {
        let mut value = context();
        if let Some(slot) = value.pointer_mut(path) {
            *slot = replacement;
        } else {
            let (parent, field) = path.rsplit_once('/').unwrap();
            value
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(field.into(), replacement);
        }
        assert!(
            rejected(&value),
            "RED: invalid checked context field must reject: {path}"
        );
    }
    let text = context().to_string();
    let duplicate = text.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
    assert!(serde_json::from_str::<LiveboardRevealContext>(&duplicate).is_err());
}

#[test]
fn receipt_identity_and_nested_context_are_strict_and_debug_is_redacted() {
    let value = json!({"version":1,"secretId":"synthetic-secret","recoveryClaimId":"synthetic-claim",
        "requesterId":"synthetic-requester","revealContext":context()});
    let receipt: LiveboardRevealReceipt = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(format!("{receipt:?}"), "LiveboardRevealReceipt([REDACTED])");
    for (field, invalid) in [
        ("version", json!(2)),
        ("requesterId", json!("foreign")),
        ("secretId", json!("")),
        ("recoveryClaimId", json!("x".repeat(1025))),
        ("revealContext", Value::Null),
    ] {
        let mut bad = value.clone();
        bad[field] = invalid;
        assert!(
            serde_json::from_str::<LiveboardRevealReceipt>(&bad.to_string()).is_err(),
            "RED: invalid checked receipt must reject: {field}"
        );
    }
}
