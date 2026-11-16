use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::DsaKeyPair;
use crate::crypto::keys::DsaPk;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::secret::{SsDistributionStatus, SsRecoveryId};
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::events::vault::device_log_event::DeviceLogObject;
use crate::node::db::objects::persistent_vault::VaultTail;
use crate::node::join_events::{JoinResultStatus, JoinResultSubscription};
use crate::node::security::canonical_json;
use crate::node::state_events::StateEventsSubscription;
use anyhow::{Result, anyhow};
use derive_more::From;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Public wire enum: keep the inline variants to preserve the existing API shape.
#[allow(clippy::large_enum_variant)]
pub enum ReadSyncRequest {
    JoinAdmission(UserData),
    Vault(VaultRequest),
    SsRequest(SsRequest),
    SsRecoveryCompletion(SignedAction),
    ServerTail(ServerTailRequest),
    StateEventsSubscription(StateEventsSubscription),
    JoinResultSubscription(JoinResultSubscription),
    LiveboardCapabilities(LiveboardCapabilitiesRequest),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Preserve existing Rust Event(SignedAction) constructors; SyncRequest::Write
// already boxes this public enum. This layout choice does not affect wire JSON.
#[allow(clippy::large_enum_variant)]
pub enum WriteSyncRequest {
    Event(SignedAction),
    DeviceTransportNoop(crate::node::device_transport_noop::DeviceTransportNoop),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncRequest {
    Read(Box<ReadSyncRequest>),
    Write(Box<WriteSyncRequest>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsRecoveryCompletion {
    pub vault_name: VaultName,
    pub recovery_id: SsRecoveryId,
    /// Status to set for receiver (Sent for accept, Declined for decline)
    #[serde(default = "default_receiver_status")]
    pub receiver_status: SsDistributionStatus,
}

/// A signed, replay-protected state-changing command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedAction {
    pub signer: DeviceId,
    /// Monotonic sequence within `stream` for this signer.
    pub nonce: u64,
    pub stream: String,
    pub signature: Base64Text,
    pub payload: SignedActionPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Public wire enum: boxing would change the serialized/public contract.
#[allow(clippy::large_enum_variant)]
pub enum SignedActionPayload {
    Event(GenericKvLogEvent),
    RecoveryCompletion(SsRecoveryCompletion),
    GuardedRemovalDecision(GuardedRemovalDecision),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedRemovalDecision {
    pub version: u32,
    pub request_id: String,
    pub event: DeviceLogObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveboardCapabilitiesRequest {
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveboardCapabilities {
    pub projection_version: u32,
    pub guarded_removal_version: u32,
    #[serde(default)]
    pub producer_correlation_version: u32,
    #[serde(default)]
    pub device_transport_noop_version: u32,
}

pub fn is_liveboard_request_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalDecisionInput {
    pub request_id: String,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalDecisionReceipt {
    pub request_id: String,
    pub event_id: ArtifactId,
    #[serde(default = "default_removal_receipt_phase")]
    pub phase: crate::node::common::model::liveboard::LiveboardOperationPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

fn default_removal_receipt_phase() -> crate::node::common::model::liveboard::LiveboardOperationPhase
{
    crate::node::common::model::liveboard::LiveboardOperationPhase::Queued
}

#[derive(Serialize)]
struct UnsignedSignedAction<'a> {
    signer: &'a DeviceId,
    nonce: u64,
    stream: &'a str,
    payload: &'a SignedActionPayload,
}

impl SignedAction {
    pub fn sign(
        payload: SignedActionPayload,
        signer: DeviceId,
        stream: String,
        nonce: u64,
        key_pair: &DsaKeyPair,
    ) -> Result<Self> {
        let mut action = Self {
            signer,
            nonce,
            stream,
            signature: Base64Text::from(""),
            payload,
        };
        let canonical = action.unsigned_canonical_bytes()?;
        action.signature = key_pair.sign(String::from_utf8(canonical)?);
        Ok(action)
    }

    pub fn unsigned_canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_json(&UnsignedSignedAction {
            signer: &self.signer,
            nonce: self.nonce,
            stream: &self.stream,
            payload: &self.payload,
        })
    }

    pub fn verify(&self, public_key: &DsaPk) -> Result<()> {
        let canonical = String::from_utf8(self.unsigned_canonical_bytes()?)?;
        public_key.verify(&canonical, &self.signature)
    }

    pub fn event(&self) -> Result<&GenericKvLogEvent> {
        match &self.payload {
            SignedActionPayload::Event(event) => Ok(event),
            SignedActionPayload::RecoveryCompletion(_)
            | SignedActionPayload::GuardedRemovalDecision(_) => {
                Err(anyhow!("signed action does not contain an event"))
            }
        }
    }

    pub fn completion(&self) -> Result<&SsRecoveryCompletion> {
        match &self.payload {
            SignedActionPayload::RecoveryCompletion(completion) => Ok(completion),
            SignedActionPayload::Event(_) | SignedActionPayload::GuardedRemovalDecision(_) => Err(
                anyhow!("signed action does not contain recovery completion"),
            ),
        }
    }
}

fn default_receiver_status() -> SsDistributionStatus {
    SsDistributionStatus::Sent
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultRequest {
    pub sender: UserData,
    pub tail: VaultTail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsRequest {
    pub sender: UserData,
    pub ss_log: ArtifactId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailRequest {
    pub sender: UserData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_reference:
        Option<crate::node::common::model::liveboard_producer::ProducerReference>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncResponse {
    Empty,
    Data(DataEventsResponse),
    ServerTailResponse(Box<ServerTailResponse>),
    Error { msg: String },
    JoinResult(JoinResultStatus),
    LiveboardCapabilities(LiveboardCapabilities),
    DeviceTransportNoop(crate::node::device_transport_noop::DeviceTransportNoopReceipt),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEventsResponse(pub Vec<GenericKvLogEvent>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTailResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_log_tail: Option<ArtifactId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ss_device_log_tail: Option<ArtifactId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_observation:
        Option<crate::node::common::model::liveboard_producer::ProducerCanonicalObservation>,
}

impl DataSyncResponse {
    pub fn ensure_success(&self) -> Result<()> {
        match self {
            DataSyncResponse::Empty => Ok(()),
            DataSyncResponse::Error { msg } => Err(anyhow!(msg.clone())),
            _ => Err(anyhow!("Invalid response type")),
        }
    }

    pub fn to_data(&self) -> Result<DataEventsResponse> {
        match self {
            DataSyncResponse::Data(data) => Ok(data.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }

    pub fn to_server_tail(&self) -> Result<ServerTailResponse> {
        match self {
            DataSyncResponse::ServerTailResponse(server_tail) => Ok(server_tail.as_ref().clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }

    pub fn to_join_result(&self) -> Result<JoinResultStatus> {
        match self {
            DataSyncResponse::JoinResult(status) => Ok(*status),
            DataSyncResponse::Error { msg } => Err(anyhow!(msg.clone())),
            _ => Err(anyhow!("Invalid response type")),
        }
    }
}
