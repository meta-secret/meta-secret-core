use serde::{Deserialize, Serialize};

use crate::crypto::keys::AeadCipherText;
use crate::sdk::password::{MetaPasswordDoc, MetaPasswordId};
use crate::sdk::vault::{UserSignature, VaultDoc};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericMessage<T> {
    pub msg_type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

impl<T> GenericMessage<T> {
    pub fn just_ok() -> Self {
        GenericMessage {
            msg_type: MessageType::Ok,
            data: None,
            err: None,
        }
    }

    pub fn data(data: T) -> Self {
        GenericMessage {
            msg_type: MessageType::Ok,
            data: Some(data),
            err: None,
        }
    }

    pub fn err(error: String) -> Self {
        GenericMessage {
            msg_type: MessageType::Err,
            data: None,
            err: Some(error),
        }
    }
}

pub type SecretDistributionDocResponse = GenericMessage<SecretDistributionDocData>;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SecretDistributionDocData {
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

pub type RegistrationResponse = GenericMessage<RegistrationStatus>;

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

pub type VaultInfoResponse = GenericMessage<VaultInfoData>;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfoData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_info: Option<VaultInfoStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultDoc>,
}

impl VaultInfoData {
    pub fn pending() -> Self {
        Self::empty(VaultInfoStatus::Pending)
    }

    pub fn declined() -> Self {
        Self::empty(VaultInfoStatus::Declined)
    }

    pub fn unknown() -> Self {
        Self::empty(VaultInfoStatus::Unknown)
    }

    pub fn empty(vault_info: VaultInfoStatus) -> Self {
        Self {
            vault_info: Some(vault_info),
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

pub type MetaPasswordsResponse = GenericMessage<MetaPasswordsData>;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetaPasswordsData {
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
pub enum MessageType {
    Ok,
    Err,
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

pub mod basic {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MongoDbStats {
        pub connection: bool,
        pub registrations: usize,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct HttpStatusDetails {
        pub http_status: String,
        pub http_status_code: u16,
        pub uri: String,
        pub method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub content_type: Option<String>,
    }
}

pub mod membership {
    use serde::{Deserialize, Serialize};

    use super::GenericMessage;

    #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub enum MembershipStatus {
        VaultNotFound,
        /// Device is a member of a vault already
        AlreadyMember,
        /// Operation finished successfully
        Finished,
    }

    pub type MembershipResponse = GenericMessage<MembershipStatus>;
}

#[cfg(test)]
mod test {
    use super::membership::*;
    use crate::sdk::api::GenericMessage;
    use serde::{Deserialize, Serialize};

    #[test]
    fn test_generic_message() {
        let msg: GenericMessage<String> = GenericMessage::just_ok();
        let msg = serde_json::to_string(&msg).unwrap();
        assert_eq!(r#"{"msgType":"ok"}"#.to_string(), msg);

        let msg = MembershipResponse::data(MembershipStatus::AlreadyMember);
        let msg = serde_json::to_string(&msg).unwrap();
        assert_eq!(r#"{"msgType":"ok","data":"alreadyMember"}"#, msg);

        #[derive(Debug, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TestTest {
            pub xxx: String,
        }

        let msg = GenericMessage::data(TestTest { xxx: "yay".to_string() });
        let msg = serde_json::to_string(&msg).unwrap();
        assert_eq!(r#"{"msgType":"ok","data":{"xxx":"yay"}}"#, msg);
    }
}
