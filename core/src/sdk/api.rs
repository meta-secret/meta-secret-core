use crate::crypto::keys::AeadCipherText;
use crate::sdk::password::{MetaPasswordDoc, MetaPasswordId};
use crate::sdk::vault::{UserSignature, VaultDoc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SecretDistributionDoc {
    pub distribution_type: SecretDistributionType,
    pub meta_password: MetaPasswordRequest,
    pub secret_message: EncryptedMessage,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SecretDistributionType {
    Split,
    Recover,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationResponse {
    pub status: MessageStatus,
    pub registration: Option<RegistrationStatus>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationStatus {
    Registered,
    AlreadyExists,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequest {
    pub member: UserSignature,
    pub candidate: UserSignature,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedMessage {
    /// Massage receiver who can decrypt message. We can't use a receiver from inside AeadCipherText because it's static
    /// and we can't know if a receiver send message back or it's the sender sending message.
    pub receiver: UserSignature,
    /// Message text encrypted with receivers' RSA public key
    pub encrypted_text: AeadCipherText,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub status: MessageStatus,
    pub vault_info: VaultInfoStatus,
    pub vault: Option<VaultDoc>,
}

impl VaultInfo {
    pub fn pending() -> Self {
        VaultInfo::empty(VaultInfoStatus::Pending)
    }

    pub fn declined() -> Self {
        VaultInfo::empty(VaultInfoStatus::Declined)
    }

    pub fn unknown() -> VaultInfo {
        VaultInfo::empty(VaultInfoStatus::Unknown)
    }

    pub fn empty(vault_info: VaultInfoStatus) -> Self {
        VaultInfo {
            status: MessageStatus::Ok,
            vault_info,
            vault: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VaultInfoStatus {
    /// Device is a member of a vault
    Member,
    /// Device is waiting to be added to a vault
    Pending,
    /// Vault members declined to add a device into the vault
    Declined,
    /// Device can't get any information about the vault, because its signature is not in members or pending list
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordsResponse {
    pub status: MessageStatus,
    pub password_status: MetaPasswordsStatus,
    pub passwords: Vec<MetaPasswordDoc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordRequest {
    //Creator of the meta password record
    pub user_sig: UserSignature,
    //meta information about password
    pub meta_password: MetaPasswordDoc,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MetaPasswordsStatus {
    Ok,
    VaultNotFound,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MessageStatus {
    Ok,
    Error { err: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRecoveryRequest {
    pub id: MetaPasswordId,
    // The device that needs data ("consumer" - the device that asks provider to provide data)
    pub consumer: UserSignature,
    //The device that has data and must provide data to consumer device
    pub provider: UserSignature,
}