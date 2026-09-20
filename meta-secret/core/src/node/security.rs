//! Authenticated protocol actions shared by Core clients and the server.

use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::DsaKeyPair;
use crate::crypto::keys::DsaPk;
use crate::node::api::SsRecoveryCompletion;
use crate::node::api::{SignedAction, SignedActionPayload};
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::IdString;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::generic_log_event::ObjIdExtractor;
use anyhow::{anyhow, Result};

/// Build the canonical stream name used by the monotonic event sequence.
pub fn event_stream(event: &GenericKvLogEvent) -> String {
    event.obj_id().fqdn.id_str()
}

/// Sign a state-changing event with the local device's DSA key.
pub fn sign_event(
    event: GenericKvLogEvent,
    signer: DeviceId,
    key_pair: &DsaKeyPair,
) -> Result<SignedAction> {
    let object_id = event.obj_id();
    let nonce = object_id.id.curr as u64;
    SignedAction::sign(
        SignedActionPayload::Event(event),
        signer,
        event_stream_from_object_id(&object_id),
        nonce,
        key_pair,
    )
}

fn event_stream_from_object_id(
    object_id: &crate::node::db::events::object_id::ArtifactId,
) -> String {
    object_id.fqdn.clone().id_str()
}

/// Verify a signature against a public DSA key.
pub fn verify_action(action: &SignedAction, public_key: &DsaPk) -> Result<()> {
    action.verify(public_key)
}

/// Completion is terminal for one recovery stream, so its first command uses
/// nonce 1 and a stream derived from the exact recovery identity.
pub fn sign_recovery_completion(
    completion: SsRecoveryCompletion,
    signer: DeviceId,
    key_pair: &DsaKeyPair,
) -> Result<SignedAction> {
    let stream = completion.recovery_id.clone().id_str();
    SignedAction::sign(
        SignedActionPayload::RecoveryCompletion(completion),
        signer,
        stream,
        1,
        key_pair,
    )
}

/// Deterministically serialize JSON with object keys sorted lexicographically.
/// This avoids signing HashMap iteration order.
pub fn canonical_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    let mut out = String::new();
    write_canonical_json(&value, &mut out)?;
    Ok(out.into_bytes())
}

fn write_canonical_json(value: &serde_json::Value, out: &mut String) -> Result<()> {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        serde_json::Value::Number(value) => out.push_str(&value.to_string()),
        serde_json::Value::String(value) => out.push_str(&serde_json::to_string(value)?),
        serde_json::Value::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical_json(value, out)?;
            }
            out.push(']');
        }
        serde_json::Value::Object(values) => {
            out.push('{');
            let mut keys: Vec<&String> = values.keys().collect();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key)?);
                out.push(':');
                write_canonical_json(&values[key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

pub fn signature_text(action: &SignedAction) -> Result<String> {
    let bytes = action.unsigned_canonical_bytes()?;
    String::from_utf8(bytes).map_err(|err| anyhow!(err))
}

pub fn signature_from_text(text: &str) -> Base64Text {
    Base64Text::from(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::utils::Id48bit;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::api::SsRecoveryCompletion;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::secret::{
        ClaimId, SsDistributionId, SsDistributionStatus, SsRecoveryId,
    };
    use crate::node::common::model::user::common::UserDataMember;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::vault::vault_event::VaultObject;

    #[test]
    fn signed_action_roundtrip_and_tamper_detection() {
        let registry = FixtureRegistry::empty();
        let creds = registry.state.user_creds.client;
        let user = creds.user();
        let key_manager = creds.device_creds.key_manager().unwrap();
        let event = GenericKvLogEvent::Vault(VaultObject::sign_up(
            VaultName::test(),
            UserDataMember::from(user.clone()),
        ));

        let stream = event_stream(&event);
        assert!(!stream.is_empty());
        let action = sign_event(event, user.device.device_id.clone(), &key_manager.dsa).unwrap();
        assert!(action.verify(&user.device.keys.dsa_pk).is_ok());
        assert!(verify_action(&action, &user.device.keys.dsa_pk).is_ok());
        assert!(signature_text(&action).is_ok());
        assert!(!signature_from_text("signed").base64_str().is_empty());
        assert!(action.event().is_ok());
        assert!(action.completion().is_err());

        let mut tampered = action.clone();
        tampered.nonce += 1;
        assert!(tampered.verify(&user.device.keys.dsa_pk).is_err());

        let other = registry.state.user_creds.client_b;
        assert!(action
            .verify(&other.device_creds.device.keys.dsa_pk)
            .is_err());

        let pass_id = MetaPasswordId::build_from_str("signature-test");
        let recovery = SsRecoveryCompletion {
            vault_name: VaultName::test(),
            recovery_id: SsRecoveryId {
                claim_id: crate::node::common::model::secret::SsClaimId {
                    id: ClaimId(Id48bit::generate()),
                    pass_id: pass_id.clone(),
                },
                sender: user.device.device_id.clone(),
                distribution_id: SsDistributionId {
                    pass_id,
                    receiver: user.device.device_id.clone(),
                },
            },
            receiver_status: SsDistributionStatus::Sent,
        };
        let completion =
            sign_recovery_completion(recovery, user.device.device_id.clone(), &key_manager.dsa)
                .unwrap();
        assert!(completion.completion().is_ok());
        assert!(completion.event().is_err());

        let canonical = canonical_json(&serde_json::json!({
            "z": [null, true, 3],
            "a": {"b": "x", "a": 1}
        }))
        .unwrap();
        assert_eq!(
            String::from_utf8(canonical).unwrap(),
            r#"{"a":{"a":1,"b":"x"},"z":[null,true,3]}"#
        );
    }
}
