//! Additive operation identity returned to Liveboard adapters.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveboardOperationKind {
    Join,
    Create,
    Recover,
    DeleteSecret,
    RemoveDevice,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveboardOperationPhase {
    Queued,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveboardOperationReceipt {
    pub kind: LiveboardOperationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub phase: LiveboardOperationPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl LiveboardOperationReceipt {
    pub(crate) fn queued(kind: LiveboardOperationKind) -> Self {
        Self {
            kind,
            secret_id: None,
            claim_id: None,
            request_id: None,
            phase: LiveboardOperationPhase::Queued,
            error_code: None,
        }
    }
    pub(crate) fn unknown(mut self, code: &'static str) -> Self {
        self.phase = LiveboardOperationPhase::Unknown;
        self.error_code = Some(code.into());
        self
    }
    pub(crate) fn require_queued(self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.phase == LiveboardOperationPhase::Queued,
            "OperationOutcomeUnknown"
        );
        Ok(())
    }
}

pub use super::liveboard_state::{LiveboardOperationState, LiveboardSecretState, LiveboardState};
