//! Local-only frozen preparation and compact transport identity.
//! These types do not prepare, publish, sign or dispatch an operation.
use super::{
    generic_log_event::{GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent},
    kv_log_event::{KvKey, KvLogEvent},
    object_id::ArtifactId,
    shared_secret_event::SsWorkflowObject,
    vault::vault_log_event::RemoveMemberEvent,
};
use crate::{
    crypto::encoding::base64::Base64Text,
    node::{
        common::model::{
            device::common::DeviceId,
            meta_pass::MetaPasswordId,
            secret::{SsClaim, SsClaimId},
            user::common::{UserData, UserId},
            vault::vault::VaultName,
        },
        device_transport_noop::{DeviceTransportNoop, DeviceTransportNoopReceipt},
    },
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedPreparationIndexObject(pub KvLogEvent<LocalGuardedPreparationIndex>);

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedPreparationObject(pub KvLogEvent<LocalGuardedPreparation>);

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedPreparationIndex {
    pub version: u32,
    pub owner: UserId,
    pub request_ids: Vec<String>,
    pub revision: u64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedPreparation {
    pub version: u32,
    pub owner: UserId,
    pub request_id: String,
    pub base_vault: ArtifactId,
    pub requester_id: DeviceId,
    pub target_id: DeviceId,
    pub survivors: Vec<UserData>,
    pub phase: GuardedPreparationPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<GuardedPreparationPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<GuardedCommandOrigin>,
    pub claims: Vec<GuardedClaimOwnership>,
    pub packets: Vec<GuardedPacketOwnership>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_noop: Option<LocalGuardedTransportNoop>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_attempts: Vec<GuardedCompactAttempt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardedPreparationPhase {
    Staged,
    Queued,
    Signed,
    AcceptedPromoting,
    AcceptedPublished,
    RetiredUnqueued,
    BlockedRetiredCommand,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedPreparationPayload {
    pub update: RemoveMemberEvent,
    pub passes: Vec<GuardedPreparedPass>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedPreparedPass {
    pub pass_id: MetaPasswordId,
    pub input: GuardedInputRef,
    pub claim: SsClaim,
    pub distributions: Vec<SsWorkflowObject>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedInputRef {
    pub distribution_id: ArtifactId,
    pub distribution_commitment: String,
    pub claim_id: SsClaimId,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedClaimOwnership {
    pub pass_id: MetaPasswordId,
    pub claim: SsClaim,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ss_device_event_id: Option<ArtifactId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ss_log_event_id: Option<ArtifactId>,
    pub handed_off: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedPacketOwnership {
    pub event_id: ArtifactId,
    pub receiver_id: DeviceId,
    pub body_commitment: String,
    pub handed_off: bool,
    pub delivered: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedCommandOrigin {
    pub event_id: ArtifactId,
    pub actor_id: DeviceId,
    pub vault_name: VaultName,
    pub request_id: String,
    pub target_id: DeviceId,
    pub accepted: bool,
    pub body_commitment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_signature: Option<GuardedBusinessSignature>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedBusinessSignature {
    pub signer: DeviceId,
    pub stream: String,
    pub nonce: u64,
    pub signature: Base64Text,
    pub signed_action_hash: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedTransportNoop {
    pub authorization: DeviceTransportNoop,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<DeviceTransportNoopReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_next_tail: Option<ArtifactId>,
}

/// Previous attempts retain identity/ownership only, never a packet-bearing payload.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardedCompactAttempt {
    pub origin: GuardedCommandOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_noop: Option<LocalGuardedTransportNoop>,
    pub phase: GuardedPreparationPhase,
    pub base_vault: ArtifactId,
    pub requester_id: DeviceId,
    pub target_id: DeviceId,
    pub survivors: Vec<UserData>,
    pub claims: Vec<GuardedClaimOwnership>,
    pub packets: Vec<GuardedPacketOwnership>,
}

// Generic envelopes derive Debug. Never expose private payloads or signatures via it,
// including when an individual nested container is logged directly.
macro_rules! redacted_debug {
    ($($model:ty),+ $(,)?) => {$(
        impl std::fmt::Debug for $model {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($model)).finish_non_exhaustive()
            }
        }
    )+};
}
redacted_debug!(
    LocalGuardedPreparationIndexObject,
    LocalGuardedPreparationObject,
    LocalGuardedPreparationIndex,
    LocalGuardedPreparation,
    GuardedPreparationPayload,
    GuardedPreparedPass,
    GuardedInputRef,
    GuardedClaimOwnership,
    GuardedPacketOwnership,
    GuardedCommandOrigin,
    GuardedBusinessSignature,
    LocalGuardedTransportNoop,
    GuardedCompactAttempt,
);

impl ToGenericEvent for LocalGuardedPreparationIndexObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::LocalGuardedPreparationIndex(self)
    }
}
impl TryFrom<GenericKvLogEvent> for LocalGuardedPreparationIndexObject {
    type Error = anyhow::Error;
    fn try_from(event: GenericKvLogEvent) -> anyhow::Result<Self> {
        match event {
            GenericKvLogEvent::LocalGuardedPreparationIndex(value) => Ok(value),
            _ => anyhow::bail!("not a local guarded preparation index"),
        }
    }
}
impl KeyExtractor for LocalGuardedPreparationIndexObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}
impl ObjIdExtractor for LocalGuardedPreparationIndexObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

impl ToGenericEvent for LocalGuardedPreparationObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::LocalGuardedPreparation(self)
    }
}
impl TryFrom<GenericKvLogEvent> for LocalGuardedPreparationObject {
    type Error = anyhow::Error;
    fn try_from(event: GenericKvLogEvent) -> anyhow::Result<Self> {
        match event {
            GenericKvLogEvent::LocalGuardedPreparation(value) => Ok(value),
            _ => anyhow::bail!("not a local guarded preparation record"),
        }
    }
}
impl KeyExtractor for LocalGuardedPreparationObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}
impl ObjIdExtractor for LocalGuardedPreparationObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}
