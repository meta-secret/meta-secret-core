use crate::node::common::model::secret::{SsDistributionStatus, SsRecoveryId};
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::object_id::ArtifactId;
use crate::node::db::objects::persistent_vault::VaultTail;
use crate::node::state_events::StateEventsSubscription;
use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::DsaKeyPair;
use crate::crypto::keys::DsaPk;
use crate::node::common::model::device::common::DeviceId;
use crate::node::security::canonical_json;
use anyhow::{anyhow, Result};
use derive_more::From;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Public wire enum: keep the inline variants to preserve the existing API shape.
#[allow(clippy::large_enum_variant)]
pub enum ReadSyncRequest {
    Vault(VaultRequest),
    SsRequest(SsRequest),
    SsRecoveryCompletion(SignedAction),
    ServerTail(ServerTailRequest),
    StateEventsSubscription(StateEventsSubscription),
}

#[derive(Clone, Debug, PartialEq, From, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WriteSyncRequest {
    Event(SignedAction),
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
            SignedActionPayload::RecoveryCompletion(_) => {
                Err(anyhow!("signed action does not contain an event"))
            }
        }
    }

    pub fn completion(&self) -> Result<&SsRecoveryCompletion> {
        match &self.payload {
            SignedActionPayload::RecoveryCompletion(completion) => Ok(completion),
            SignedActionPayload::Event(_) => {
                Err(anyhow!("signed action does not contain recovery completion"))
            }
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSyncResponse {
    Empty,
    Data(DataEventsResponse),
    ServerTailResponse(ServerTailResponse),
    Error { msg: String },
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
            DataSyncResponse::ServerTailResponse(server_tail) => Ok(server_tail.clone()),
            _ => Err(anyhow!("Invalid response type")),
        }
    }
}
