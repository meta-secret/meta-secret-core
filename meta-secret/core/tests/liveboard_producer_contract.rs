//! Typed parser/wire tests only. Source references are input, never authority.
//! Actual producer/server/save behavior is covered in separate boundary targets.
use anyhow::{Context, Result};
use meta_secret_core::{
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        common::model::{
            liveboard_producer::*, meta_pass::MetaPasswordId, user::common::UserDataMember,
            vault::vault_data::VaultData,
        },
        db::{
            descriptors::vault_descriptor::DeviceLogDescriptor,
            events::{object_id::ArtifactId, vault::vault_log_event::DeleteSecretRequest},
        },
    },
};
use serde_json::{Value, json};

fn reference() -> ProducerReference {
    let actor = FixtureRegistry::empty().state.user_creds.client;
    let member = UserDataMember::from(actor.user());
    let pass = MetaPasswordId::build_from_str("parser-only-secret-identity");
    let vault = VaultData::from(member.clone()).add_secret(pass.clone());
    // The actual Delete builder allocates this UUID; no client guesses it.
    let request = DeleteSecretRequest::new(member, pass, &vault);
    ProducerReference {
        version: 1,
        kind: ProducerKind::DeleteSecret,
        request_id: Some(request.request_id),
        request_stage: ProducerSourceRef {
            event_id: ArtifactId::from(DeviceLogDescriptor::from(actor.user_id())),
            // Parser-only grammar input; it grants no admission/ownership claim.
            body_commitment: "ab".repeat(32),
        },
        decision_stage: None,
    }
}

#[test]
fn producer_reference_accepts_complete_delete_uuid_and_full_artifact_control() -> Result<()> {
    let input = reference();
    let json = serde_json::to_string(&input)?;
    let parsed = ProducerReference::parse_json(&json)
        .context("RED: valid exact typed reference parser unavailable")?;
    assert_eq!(parsed, input);
    assert_eq!(
        serde_json::to_value(&parsed)?["requestStage"]["eventId"]["id"],
        json!({"curr":1,"prev":0})
    );
    Ok(())
}

#[test]
fn producer_reference_remove_unknown_uuid_omits_it_and_preserves_large_exact_sequence() -> Result<()>
{
    let mut wire = serde_json::to_value(reference())?;
    wire["kind"] = json!("removeDevice");
    wire.as_object_mut()
        .context("reference object")?
        .remove("requestId");
    // A JSON numeric decoder may not round through f64 at the JS boundary.
    wire["requestStage"]["eventId"]["id"] =
        json!({"curr":9007199254740993u64,"prev":9007199254740992u64});
    let parsed = ProducerReference::parse_json(&wire.to_string())
        .context("RED: exact full sequence must not be truncated")?;
    assert_eq!(parsed.request_id, None);
    let encoded = serde_json::to_value(parsed)?;
    assert!(encoded.get("requestId").is_none());
    assert_eq!(
        encoded["requestStage"]["eventId"]["id"],
        wire["requestStage"]["eventId"]["id"]
    );
    Ok(())
}

#[test]
fn producer_reference_rejects_unknown_or_duplicate_fields_at_every_nested_level() -> Result<()> {
    let original = serde_json::to_value(reference())?;
    for path in [
        "",
        "/requestStage",
        "/requestStage/eventId",
        "/requestStage/eventId/fqdn",
        "/requestStage/eventId/id",
    ] {
        let mut changed = original.clone();
        changed
            .pointer_mut(path)
            .context("actual nested wire object")?
            .as_object_mut()
            .context("nested object")?
            .insert("unknownField".into(), json!(true));
        assert!(
            ProducerReference::parse_json(&changed.to_string()).is_err(),
            "unknown nested field accepted at {path}"
        );
    }
    let json = original.to_string();
    for (key, value) in [
        ("version", "1"),
        ("bodyCommitment", "\"PRIVATE-DUPLICATE-MARKER\""),
        ("curr", "1"),
        ("prev", "0"),
        ("objType", "\"PRIVATE-DUPLICATE-MARKER\""),
    ] {
        let needle = format!("\"{key}\":");
        let duplicate = json.replacen(&needle, &format!("\"{key}\":{value},{needle}"), 1);
        assert_ne!(
            json, duplicate,
            "fixture must actually insert duplicate field"
        );
        assert!(
            ProducerReference::parse_json(&duplicate).is_err(),
            "duplicate {key} accepted"
        );
    }
    Ok(())
}

#[test]
fn producer_reference_rejects_bad_sequence_commitment_authority_and_unbounded_input() -> Result<()>
{
    let original = serde_json::to_value(reference())?;
    let changes = [
        ("/version", json!(2)),
        ("/kind", json!("DeleteSecret")),
        ("/requestId", json!("not-a-uuid")),
        ("/requestId", Value::Null),
        ("/requestStage/eventId/id/curr", json!(0)),
        ("/requestStage/eventId/id/curr", json!(1.5)),
        ("/requestStage/eventId/id/prev", json!(8)),
        ("/requestStage/bodyCommitment", json!("AB".repeat(32))),
        ("/requestStage/bodyCommitment", json!("ab".repeat(31))),
        ("/requestStage/source", Value::Null),
    ];
    for (path, value) in changes {
        let mut changed = original.clone();
        if let Some(target) = changed.pointer_mut(path) {
            *target = value;
        } else {
            changed["requestStage"]["source"] = value;
        }
        assert!(
            ProducerReference::parse_json(&changed.to_string()).is_err(),
            "invalid reference accepted at {path}"
        );
    }
    for field in [
        "readiness",
        "observation",
        "materialStage",
        "biometry",
        "accepted",
    ] {
        let mut changed = original.clone();
        changed[field] = json!("caller-cannot-authorize");
        assert!(ProducerReference::parse_json(&changed.to_string()).is_err());
    }
    for input in [String::new(), " ".into(), "x".repeat(4097)] {
        let error = ProducerReference::parse_json(&input).expect_err("invalid input");
        assert!(!error.to_string().contains(&input) || input.trim().is_empty());
    }
    Ok(())
}

#[test]
fn producer_wire_exact_not_attempted_and_debug_are_redacted_controls() -> Result<()> {
    let source = reference().request_stage;
    let stage = ProducerAttemptStage {
        source: source.clone(),
        observation: ProducerSourceObservation::Unconfirmed,
    };
    let receipt = LiveboardProducerReceipt {
        version: 1,
        kind: ProducerKind::RemoveDevice,
        phase: ProducerPhase::Unknown,
        secret_id: None,
        target_device_id: Some("PRIVATE-DEVICE-MARKER".into()),
        request_id: None,
        request_stage: stage.clone(),
        decision_stage: Some(ProducerDecisionStage::NotAttempted),
        material_stage: Some(ProducerMaterialStage::NotStarted),
        canonical_state: ProducerCanonicalState::Unknown,
        readiness: "synchronizing".into(),
        error_code: Some("OperationUnavailable".into()),
    };
    let wire = serde_json::to_value(&receipt)?;
    assert_eq!(wire["decisionStage"], json!({"state":"notAttempted"}));
    assert!(wire.get("requestId").is_none());
    assert!(wire.get("secretId").is_none());
    assert_eq!(
        serde_json::from_value::<LiveboardProducerReceipt>(wire)?,
        receipt
    );
    let observation = ProducerCanonicalObservation {
        version: 1,
        kind: ProducerKind::RemoveDevice,
        secret_id: None,
        target_device_id: None,
        request_id: None,
        request_stage: stage,
        decision_stage: None,
        canonical_state: ProducerCanonicalState::Unknown,
    };
    let debug = format!("{source:?} {receipt:?} {observation:?}");
    for forbidden in [
        "PRIVATE-DEVICE-MARKER",
        source.body_commitment.as_str(),
        source.event_id.fqdn.obj_instance.as_str(),
    ] {
        assert!(
            !debug.contains(forbidden),
            "debug must not expose producer references or payloads"
        );
    }
    assert!(
        !serde_json::to_value(observation)?
            .as_object()
            .context("observation")?
            .contains_key("readiness")
    );
    Ok(())
}
