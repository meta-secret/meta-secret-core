//! Public metadata errors are static even when strict raw decoding fails first.
use meta_secret_core::node::common::model::liveboard_reveal::{
    LiveboardRevealContext, LiveboardRevealReceipt, RevealSourceRef,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;
const CLASS: &str = "InvalidRevealMetadata";
fn source(kind: &str, name: &str) -> Value {
    json!({"eventId":{"fqdn":{"objType":kind,"objInstance":name},"id":{"curr":1,"prev":0}},
        "bodyCommitment":"a".repeat(64)})
}
fn context() -> Value {
    json!({"version":1,"vaultName":"synthetic-vault","currentDeviceId":"synthetic-requester",
        "vaultSource":source("Vault","synthetic-vault"),
        "localDistributionSource":source("SsDistribution","synthetic-local"),
        "recoverySources":[source("SsRecovery","synthetic-remote")],"claimScopeCommitment":"b".repeat(64)})
}
fn receipt() -> Value {
    json!({"version":1,"secretId":"synthetic-secret","recoveryClaimId":"synthetic-claim",
        "requesterId":"synthetic-requester","revealContext":context()})
}
struct Case {
    name: &'static str,
    raw: String,
    marker: String,
    value_check: bool,
}
fn marker(group: &str, name: &str) -> String {
    format!("CALLER-{group}-{name}-DO-NOT-ECHO")
}
fn changed(valid: &Value, path: &str, value: Value) -> Value {
    let mut copy = valid.clone();
    *copy.pointer_mut(path).expect("test mutation path exists") = value;
    copy
}
fn add(cases: &mut Vec<Case>, group: &str, name: &'static str, value: Value) {
    cases.push(Case {
        name,
        raw: value.to_string(),
        marker: marker(group, name),
        value_check: true,
    });
}
// Raw JSON intentionally preserves duplicates; never pass these through Value.
fn duplicate(valid: &Value, path: &str, field: &str, value: &str) -> String {
    let original = valid
        .pointer(path)
        .expect("duplicate target exists")
        .to_string();
    let mut repeated = original.clone();
    repeated.pop();
    repeated.push_str(&format!(",{}:{}}}", json!(field), json!(value)));
    valid.to_string().replacen(&original, &repeated, 1)
}
fn raw_cases(valid: &Value, group: &str, field: &str, src: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    for (name, path) in [
        ("root-string", String::new()),
        ("root-field-type", format!("/{field}")),
        ("source-type", src.into()),
        ("event-id-type", format!("{src}/eventId")),
        ("fqdn-type", format!("{src}/eventId/fqdn")),
        ("sequence-type", format!("{src}/eventId/id")),
        ("curr-type", format!("{src}/eventId/id/curr")),
        ("prev-type", format!("{src}/eventId/id/prev")),
        ("commitment-type", format!("{src}/bodyCommitment")),
    ] {
        let bad = if name == "commitment-type" {
            json!({(marker(group, name)):true})
        } else {
            json!(marker(group, name))
        };
        add(&mut cases, group, name, changed(valid, &path, bad));
    }
    for (name, path) in [
        ("root-unknown-key", String::new()),
        ("nested-id-unknown-key", format!("{src}/eventId")),
        ("nested-fqdn-unknown-key", format!("{src}/eventId/fqdn")),
        ("nested-sequence-unknown-key", format!("{src}/eventId/id")),
    ] {
        let mut bad = valid.clone();
        bad.pointer_mut(&path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(marker(group, name), json!(true));
        add(&mut cases, group, name, bad);
    }
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove(field);
    add(&mut cases, group, "missing-required", missing);
    add(
        &mut cases,
        group,
        "null-required",
        changed(valid, &format!("/{field}"), Value::Null),
    );
    add(&mut cases, group, "null-root", Value::Null);
    add(
        &mut cases,
        group,
        "null-source",
        changed(valid, src, Value::Null),
    );
    let sequence = format!("{src}/eventId/id");
    let fqdn = format!("{src}/eventId/fqdn");
    for (name, path, key) in [
        ("duplicate-root-key", "", field),
        ("duplicate-sequence-key", sequence.as_str(), "curr"),
        ("duplicate-fqdn-key", fqdn.as_str(), "objType"),
    ] {
        let marker = marker(group, name);
        cases.push(Case {
            name,
            raw: duplicate(valid, path, key, &marker),
            marker,
            value_check: false,
        });
    }
    cases
}
fn location(value: &str, separator: &str) -> bool {
    value.split_once(separator).is_some_and(|(line, column)| {
        [line, column]
            .iter()
            .all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
    })
}
fn static_display(value: &str) -> bool {
    value == CLASS
        || value
            .strip_prefix("InvalidRevealMetadata at line ")
            .is_some_and(|rest| location(rest, " column "))
}
fn static_debug(value: &str) -> bool {
    value
        .strip_prefix("Error(\"InvalidRevealMetadata\", line: ")
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|rest| location(rest, ", column: "))
}
fn check_error<T>(
    result: Result<T, serde_json::Error>,
    case: &Case,
    mode: &str,
    failures: &mut Vec<String>,
) {
    let Some(error) = result.err() else {
        failures.push(format!("{}:{mode}:rejected=false", case.name));
        return;
    };
    let display = error.to_string();
    let debug = format!("{error:?}");
    let display_leak = display.contains(&case.marker);
    let debug_leak = debug.contains(&case.marker);
    if display_leak || debug_leak || !static_display(&display) || !static_debug(&debug) {
        // Only authored case labels and booleans enter diagnostics, never errors/input.
        failures.push(format!("{}:{mode}:display-marker={display_leak},debug-marker={debug_leak},static-display={},static-debug={}",
            case.name, static_display(&display), static_debug(&debug)));
    }
}
fn verify<T: DeserializeOwned + Serialize + Debug>(valid: Value, group: &str, cases: Vec<Case>) {
    let decoded: T =
        serde_json::from_str(&valid.to_string()).expect("valid metadata control parses");
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        valid,
        "valid public JSON shape round-trips"
    );
    assert_eq!(format!("{decoded:?}"), format!("{group}([REDACTED])"));
    assert!(
        serde_json::from_value::<T>(valid).is_ok(),
        "valid Value control parses"
    );
    let mut failures = Vec::new();
    for case in &cases {
        check_error(
            serde_json::from_str::<T>(&case.raw),
            case,
            "string",
            &mut failures,
        );
        if case.value_check {
            let raw: Value =
                serde_json::from_str(&case.raw).expect("authored JSON syntax is valid");
            check_error(
                serde_json::from_value::<T>(raw),
                case,
                "value",
                &mut failures,
            );
        }
    }
    assert!(
        failures.is_empty(),
        "RED: {group} errors must reject with static/redacted Display and Debug: {failures:?}"
    );
}
#[test]
fn source_errors_are_static_and_redacted_before_and_after_semantic_validation() {
    let valid = source("Vault", "synthetic-vault");
    let mut cases = raw_cases(&valid, "Source", "eventId", "");
    add(
        &mut cases,
        "Source",
        "semantic-commitment",
        changed(
            &valid,
            "/bodyCommitment",
            json!(marker("Source", "semantic-commitment")),
        ),
    );
    add(
        &mut cases,
        "Source",
        "semantic-sequence",
        changed(&valid, "/eventId/id/prev", json!(9)),
    );
    verify::<RevealSourceRef>(valid, "RevealSourceRef", cases);
}
#[test]
fn context_errors_are_static_and_redacted_before_and_after_semantic_validation() {
    let valid = context();
    let mut cases = raw_cases(&valid, "Context", "version", "/vaultSource");
    add(
        &mut cases,
        "Context",
        "semantic-version",
        changed(&valid, "/version", json!(2)),
    );
    add(
        &mut cases,
        "Context",
        "semantic-bounds",
        changed(&valid, "/vaultName", json!("x".repeat(1025))),
    );
    add(
        &mut cases,
        "Context",
        "semantic-order",
        changed(
            &valid,
            "/recoverySources",
            json!([source("SsRecovery", "z"), source("SsRecovery", "a")]),
        ),
    );
    verify::<LiveboardRevealContext>(valid, "LiveboardRevealContext", cases);
}
#[test]
fn receipt_errors_are_static_and_redacted_before_and_after_semantic_validation() {
    let valid = receipt();
    let mut cases = raw_cases(&valid, "Receipt", "version", "/revealContext/vaultSource");
    add(
        &mut cases,
        "Receipt",
        "nested-context-type",
        changed(
            &valid,
            "/revealContext",
            json!(marker("Receipt", "nested-context-type")),
        ),
    );
    add(
        &mut cases,
        "Receipt",
        "nested-context-null",
        changed(&valid, "/revealContext", Value::Null),
    );
    add(
        &mut cases,
        "Receipt",
        "nested-context-version-type",
        changed(
            &valid,
            "/revealContext/version",
            json!(marker("Receipt", "nested-context-version-type")),
        ),
    );
    add(
        &mut cases,
        "Receipt",
        "semantic-requester",
        changed(
            &valid,
            "/requesterId",
            json!(marker("Receipt", "semantic-requester")),
        ),
    );
    add(
        &mut cases,
        "Receipt",
        "semantic-version",
        changed(&valid, "/version", json!(2)),
    );
    verify::<LiveboardRevealReceipt>(valid, "LiveboardRevealReceipt", cases);
}
