//! Strict wire decoding only; source ownership and admission need actual Core context.
use super::{
    LIVEBOARD_PRODUCER_VERSION, MAX_PRODUCER_REFERENCE_BYTES, ProducerKind, ProducerReference,
    ProducerSourceRef,
};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Deserializer, Serialize};

pub(super) fn parse_json(input: &str) -> Result<ProducerReference> {
    ensure!(
        !input.is_empty() && input.len() <= MAX_PRODUCER_REFERENCE_BYTES,
        "InvalidProducerReference"
    );
    let wire: ReferenceWire =
        serde_json::from_str(input).map_err(|_| anyhow!("InvalidProducerReference"))?;
    wire.into_reference()
}

pub(super) fn deserialize_reference<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ProducerReference, D::Error> {
    let wire = ReferenceWire::deserialize(deserializer)
        .map_err(|_| <D::Error as serde::de::Error>::custom("InvalidProducerReference"))?;
    let reference = wire
        .into_reference()
        .map_err(|_| serde::de::Error::custom("InvalidProducerReference"))?;
    let encoded = serde_json::to_vec(&reference)
        .map_err(|_| serde::de::Error::custom("InvalidProducerReference"))?;
    if encoded.len() > MAX_PRODUCER_REFERENCE_BYTES {
        return Err(serde::de::Error::custom("InvalidProducerReference"));
    }
    Ok(reference)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceWire {
    version: u32,
    kind: String,
    #[serde(default)]
    request_id: OptionalField<Option<String>>,
    request_stage: SourceWire,
    #[serde(default)]
    decision_stage: OptionalField<Option<SourceWire>>,
}

impl ReferenceWire {
    fn into_reference(self) -> Result<ProducerReference> {
        ensure!(
            self.version == LIVEBOARD_PRODUCER_VERSION,
            "InvalidProducerReference"
        );
        let kind = match self.kind.as_str() {
            "deleteSecret" => ProducerKind::DeleteSecret,
            "removeDevice" => ProducerKind::RemoveDevice,
            _ => return Err(anyhow!("InvalidProducerReference")),
        };
        if kind == ProducerKind::DeleteSecret {
            ensure!(
                matches!(&self.decision_stage, OptionalField::Absent),
                "InvalidProducerReference"
            );
        }
        let request_id = self.request_id.into_option().flatten();
        ensure!(
            kind != ProducerKind::DeleteSecret || request_id.is_some(),
            "InvalidProducerReference"
        );
        if let Some(id) = &request_id {
            ensure!(
                uuid::Uuid::parse_str(id)
                    .is_ok_and(|parsed| parsed.hyphenated().to_string() == *id),
                "InvalidProducerReference"
            );
        }
        Ok(ProducerReference {
            version: self.version,
            kind,
            request_id,
            request_stage: self.request_stage.into_source()?,
            decision_stage: self
                .decision_stage
                .into_option()
                .flatten()
                .map(SourceWire::into_source)
                .transpose()?,
        })
    }
}

// A non-Option field lets serde detect duplicate keys even when their value is
// null. Presence also distinguishes an omitted Delete decision from null input.
#[derive(Default)]
enum OptionalField<T> {
    #[default]
    Absent,
    Present(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OptionalField<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Present)
    }
}

impl<T> OptionalField<T> {
    fn into_option(self) -> Option<T> {
        match self {
            Self::Absent => None,
            Self::Present(value) => Some(value),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceWire {
    event_id: ArtifactWire,
    body_commitment: String,
}

impl SourceWire {
    fn into_source(self) -> Result<ProducerSourceRef> {
        ensure!(
            self.body_commitment.len() == 64
                && self
                    .body_commitment
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "InvalidProducerReference"
        );
        ensure!(
            self.event_id.id.curr.checked_sub(1) == Some(self.event_id.id.prev),
            "InvalidProducerReference"
        );
        // Typed serialization bridges the existing private SeqId fields without
        // adding a constructor or passing integer identity through f64/Value.
        let encoded =
            serde_json::to_vec(&self.event_id).map_err(|_| anyhow!("InvalidProducerReference"))?;
        let event_id: ArtifactId =
            serde_json::from_slice(&encoded).map_err(|_| anyhow!("InvalidProducerReference"))?;
        Ok(ProducerSourceRef {
            event_id,
            body_commitment: self.body_commitment,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactWire {
    fqdn: FqdnWire,
    id: SequenceWire,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FqdnWire {
    obj_type: String,
    obj_instance: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SequenceWire {
    curr: usize,
    prev: usize,
}
