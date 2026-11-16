//! Checked result metadata only; never an input granting a reveal or command.
use crate::node::db::events::object_id::ArtifactId;
use crate::secret::shared_secret::PlainText;
use serde::{Deserialize, Serialize};
use std::fmt;

#[path = "liveboard_reveal_parser.rs"]
mod strict;

pub const REVEAL_CONTEXT_MAX_BYTES: usize = 8192;
pub const REVEAL_RECEIPT_MAX_BYTES: usize = 12288;
pub const REVEAL_ID_MAX_BYTES: usize = 1024;

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevealSourceRef {
    pub event_id: ArtifactId,
    pub body_commitment: String,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveboardRevealContext {
    pub version: u32,
    pub vault_name: String,
    pub current_device_id: String,
    pub vault_source: RevealSourceRef,
    pub local_distribution_source: RevealSourceRef,
    pub recovery_sources: Vec<RevealSourceRef>,
    pub claim_scope_commitment: String,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveboardRevealReceipt {
    pub version: u32,
    pub secret_id: String,
    pub recovery_claim_id: String,
    pub requester_id: String,
    pub reveal_context: LiveboardRevealContext,
}

impl<'de> Deserialize<'de> for RevealSourceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = strict::Source::deserialize(deserializer)
            .map_err(|_| serde::de::Error::custom("InvalidRevealMetadata"))?;
        Self::try_from(raw).map_err(|_| serde::de::Error::custom("InvalidRevealMetadata"))
    }
}

impl<'de> Deserialize<'de> for LiveboardRevealContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = strict::Context::deserialize(deserializer)
            .map_err(|_| serde::de::Error::custom("InvalidRevealMetadata"))?;
        Self::try_from(raw).map_err(|_| serde::de::Error::custom("InvalidRevealMetadata"))
    }
}

impl<'de> Deserialize<'de> for LiveboardRevealReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = strict::Receipt::deserialize(deserializer)
            .map_err(|_| serde::de::Error::custom("InvalidRevealMetadata"))?;
        Self::try_from(raw).map_err(|_| serde::de::Error::custom("InvalidRevealMetadata"))
    }
}

/// Intentionally neither Debug nor Serialize: plaintext uses only the old sink.
pub struct RecoveredReveal {
    pub plaintext: PlainText,
    pub reveal_receipt: Option<LiveboardRevealReceipt>,
}

macro_rules! redacted_debug {
    ($($ty:ty),+) => {$(impl fmt::Debug for $ty {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(concat!(stringify!($ty), "([REDACTED])"))
        }
    })+};
}
redacted_debug!(
    RevealSourceRef,
    LiveboardRevealContext,
    LiveboardRevealReceipt
);

pub(crate) fn bounded_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= REVEAL_ID_MAX_BYTES
}

pub(crate) fn hex_commitment(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl RevealSourceRef {
    pub(crate) fn valid(&self, kind: &str) -> bool {
        let id = &self.event_id;
        let Ok(sequence) = serde_json::to_value(&id.id) else {
            return false;
        };
        let Some(previous) = sequence["prev"].as_u64() else {
            return false;
        };
        id.fqdn.obj_type == kind
            && bounded_id(&id.fqdn.obj_instance)
            && id.id.curr > 0
            && id.id.curr as u64 <= 9_007_199_254_740_991
            && previous.checked_add(1) == Some(id.id.curr as u64)
            && hex_commitment(&self.body_commitment)
    }
}

impl LiveboardRevealContext {
    pub fn is_valid(&self) -> bool {
        self.version == 1
            && bounded_id(&self.vault_name)
            && bounded_id(&self.current_device_id)
            && self.vault_source.valid("Vault")
            && self.local_distribution_source.valid("SsDistribution")
            && (1..=2).contains(&self.recovery_sources.len())
            && self
                .recovery_sources
                .iter()
                .all(|source| source.valid("SsRecovery"))
            && self
                .recovery_sources
                .windows(2)
                .all(|pair| source_order(&pair[0]) < source_order(&pair[1]))
            && hex_commitment(&self.claim_scope_commitment)
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= REVEAL_CONTEXT_MAX_BYTES)
    }
}

pub(crate) fn source_order(source: &RevealSourceRef) -> (&str, &str, usize) {
    (
        &source.event_id.fqdn.obj_type,
        &source.event_id.fqdn.obj_instance,
        source.event_id.id.curr,
    )
}

impl LiveboardRevealReceipt {
    pub fn is_valid(&self) -> bool {
        self.version == 1
            && bounded_id(&self.secret_id)
            && bounded_id(&self.recovery_claim_id)
            && bounded_id(&self.requester_id)
            && self.requester_id == self.reveal_context.current_device_id
            && self.reveal_context.is_valid()
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= REVEAL_RECEIPT_MAX_BYTES)
    }
}
