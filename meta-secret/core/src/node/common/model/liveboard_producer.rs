//! Typed producer metadata and bounded reference grammar; no producer/query behavior.
use crate::node::db::events::object_id::ArtifactId;
use serde::{Deserialize, Serialize};

pub use crate::node::db::events::vault::producer_metadata::producer_body_commitment as producer_source_commitment;

#[path = "liveboard_producer_parser.rs"]
mod reference_parser;

pub const LIVEBOARD_PRODUCER_VERSION: u32 = 1;
pub const MAX_PRODUCER_REFERENCE_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerKind {
    DeleteSecret,
    RemoveDevice,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerPhase {
    Queued,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerSourceObservation {
    Unconfirmed,
    Persisted,
    Admitted,
    RawAcknowledged,
    Absent,
    Redacted,
    Conflict,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerCanonicalState {
    Unknown,
    Pending,
    Prepared,
    Applied,
    Declined,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProducerMaterialStage {
    NotStarted,
    Pending,
    Ready,
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProducerSourceRef {
    pub event_id: ArtifactId,
    pub body_commitment: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProducerAttemptStage {
    pub source: ProducerSourceRef,
    pub observation: ProducerSourceObservation,
}

// Exact notAttempted JSON is {"state":"notAttempted"}.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProducerDecisionStage {
    NotAttempted,
    Attempted {
        source: ProducerSourceRef,
        observation: ProducerSourceObservation,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveboardProducerReceipt {
    pub version: u32,
    pub kind: ProducerKind,
    pub phase: ProducerPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub request_stage: ProducerAttemptStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_stage: Option<ProducerDecisionStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_stage: Option<ProducerMaterialStage>,
    pub canonical_state: ProducerCanonicalState,
    pub readiness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// Distinct strict INPUT, never deserialize a receipt as authority.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProducerReference {
    pub version: u32,
    pub kind: ProducerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub request_stage: ProducerSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_stage: Option<ProducerSourceRef>,
}

// Server observation deliberately has no local materialStage/readiness/phase.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProducerCanonicalObservation {
    pub version: u32,
    pub kind: ProducerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub request_stage: ProducerAttemptStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_stage: Option<ProducerAttemptStage>,
    pub canonical_state: ProducerCanonicalState,
}

macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($ty), "(<redacted>)"))
            }
        }
    )+};
}
redacted_debug!(
    ProducerSourceRef,
    ProducerAttemptStage,
    ProducerDecisionStage,
    LiveboardProducerReceipt,
    ProducerReference,
    ProducerCanonicalObservation
);

impl ProducerReference {
    pub fn parse_json(input: &str) -> anyhow::Result<Self> {
        reference_parser::parse_json(input)
    }
}

impl<'de> Deserialize<'de> for ProducerReference {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        reference_parser::deserialize_reference(deserializer)
    }
}
