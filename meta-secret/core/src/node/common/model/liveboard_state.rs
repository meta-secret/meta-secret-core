//! Public presentation metadata only. These hints never authorize a command.
use super::liveboard::LiveboardOperationKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveboardState {
    pub schema_version: u8,
    pub vault_name: String,
    pub current_device_id: String,
    pub storage_mode: String,
    pub can_create_secret: bool,
    pub removable_device_ids: Vec<String>,
    pub secrets: Vec<LiveboardSecretState>,
    pub operations: Vec<LiveboardOperationState>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveboardSecretState {
    pub secret_id: String,
    pub primary_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_claim_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_request_id: Option<String>,
    pub can_delete: bool,
    pub readiness: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveboardOperationState {
    pub kind: LiveboardOperationKind,
    pub request_id: String,
    pub requester_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<String>,
    pub client_status: String,
    pub eligible_peer_ids: Vec<String>,
    pub can_approve: bool,
    pub can_decline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responder_id: Option<String>,
    pub resolution_source: String,
    pub readiness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reveal_context: Option<super::liveboard_reveal::LiveboardRevealContext>,
}
impl LiveboardOperationState {
    pub(crate) fn pending(kind: LiveboardOperationKind, id: String, requester: String) -> Self {
        Self {
            kind,
            request_id: id,
            requester_id: requester,
            target_device_id: None,
            secret_id: None,
            client_status: "pending".into(),
            eligible_peer_ids: vec![],
            can_approve: false,
            can_decline: false,
            outcome: None,
            responder_id: None,
            resolution_source: "unknown".into(),
            readiness: "waiting".into(),
            reveal_context: None,
        }
    }
    pub(crate) fn eligibility(&mut self, current: &str, mut peers: Vec<String>) {
        peers.sort();
        peers.dedup();
        self.can_approve = peers.iter().any(|peer| peer == current);
        self.can_decline = self.can_approve;
        if self.can_approve {
            self.client_status = "needApprove".into();
        }
        self.eligible_peer_ids = peers;
    }
}
