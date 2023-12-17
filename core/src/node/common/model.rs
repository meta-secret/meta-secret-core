use std::fmt::Display;

use crate::node::common::model::crypto::EncryptedMessage;
use crate::node::common::model::device::{DeviceData, DeviceLink};
use crate::node::common::model::user::UserId;
use crate::node::common::model::vault::{VaultName, VaultStatus};
use rand::{distributions::Alphanumeric, Rng};
use crate::crypto::utils;
use crate::node::common::model::secret::MetaPasswordId;

pub mod device {
    use std::fmt::Display;
    use anyhow::anyhow;

    use crypto::utils::generate_uuid_b64_url_enc;

    use crate::crypto;
    use crate::crypto::keys::{KeyManager, OpenBox};
    use crate::crypto::keys::SecretBox;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceId(String);

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceName(String);

    impl From<String> for DeviceName {
        fn from(device_name: String) -> Self {
            DeviceName(device_name)
        }
    }

    impl From<&str> for DeviceName {
        fn from(device_name: &str) -> Self {
            DeviceName(String::from(device_name))
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum DeviceLink {
        Loopback(LoopbackDeviceLink),
        PeerToPeer(PeerToPeerDeviceLink),
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LoopbackDeviceLink {
        device: DeviceId
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PeerToPeerDeviceLink {
        sender: DeviceId,
        receiver: DeviceId
    }

    pub struct DeviceLinkBuilder {
        sender: Option<DeviceId>,
        receiver: Option<DeviceId>
    }

    impl DeviceLinkBuilder {
        pub fn new() -> Self {
            Self { sender: None, receiver: None }
        }

        pub fn sender(mut self, sender: DeviceId) -> Self {
            self.sender = Some(sender);
            self
        }

        pub fn receiver(mut self, receiver: DeviceId) -> Self {
            self.receiver = Some(receiver);
            self
        }

        pub fn build(self) -> anyhow::Result<DeviceLink> {
            let sender = self.sender.ok_or(anyhow!("Sender is not set"))?;

            let device_link = if let Some(receiver) = self.receiver {
                if sender == receiver {
                    DeviceLink::Loopback(LoopbackDeviceLink { device: sender } )
                } else {
                    DeviceLink::PeerToPeer(PeerToPeerDeviceLink { sender, receiver })
                }
            } else {
                DeviceLink::Loopback(LoopbackDeviceLink { device: sender } )
            };

            Ok(device_link)
        }
    }

    impl DeviceLink {
        pub fn is_loopback(&self) -> bool {
            matches!(self, DeviceLink::Loopback(_))
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceData {
        pub id: DeviceId,
        pub name: DeviceName,
        pub keys: OpenBox,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceCredentials {
        pub secret_box: SecretBox,
        pub device: DeviceData,
    }

    /// Contains full information about device (private keys and device id)
    impl DeviceCredentials {
        pub fn generate(device_name: DeviceName) -> DeviceCredentials {
            let secret_box = KeyManager::generate_secret_box();
            let device = DeviceData::from(device_name, OpenBox::from(&secret_box));
            DeviceCredentials { secret_box, device }
        }
    }

    impl Display for DeviceId {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0.clone())
        }
    }

    /// Contains only public information about device
    impl DeviceData {
        pub fn from(device_name: DeviceName, open_box: OpenBox) -> Self {
            Self {
                name: device_name,
                id: DeviceId::from(&open_box),
                keys: open_box,
            }
        }
    }

    impl From<&OpenBox> for DeviceId {
        fn from(open_box: &OpenBox) -> Self {
            let dsa_pk = open_box.dsa_pk.base64_text.clone();
            let id = generate_uuid_b64_url_enc(dsa_pk);
            Self(id)
        }
    }
}

pub mod user {
    use crate::node::common::model::device::{DeviceCredentials, DeviceData, DeviceId, DeviceName};
    use crate::node::common::model::vault::VaultName;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserId {
        pub vault_name: VaultName,
        pub device_id: DeviceId,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserData {
        pub vault_name: VaultName,
        pub device: DeviceData,
    }

    impl UserData {
        pub fn user_id(&self) -> UserId {
            UserId {
                vault_name: self.vault_name.clone(),
                device_id: self.device.id.clone(),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserCredentials {
        pub vault_name: VaultName,
        pub device_creds: DeviceCredentials,
    }

    impl UserCredentials {
        pub fn from(device_creds: DeviceCredentials, vault_name: VaultName) -> UserCredentials {
            UserCredentials { vault_name, device_creds }
        }

        pub fn generate(device_name: DeviceName, vault_name: VaultName) -> UserCredentials {
            UserCredentials {
                vault_name,
                device_creds: DeviceCredentials::generate(device_name),
            }
        }

        pub fn device(&self) -> DeviceData {
            self.device_creds.device.clone()
        }

        pub fn user(&self) -> UserData {
            UserData {
                vault_name: self.vault_name.clone(),
                device: self.device(),
            }
        }

        pub fn user_id(&self) -> UserId {
            UserId {
                vault_name: self.vault_name.clone(),
                device_id: self.device().id.clone(),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum UserMembership {
        Outsider(UserDataOutsider),
        Member(UserDataMember),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDataMember {
        pub user_data: UserData,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserDataOutsider {
        pub user_data: UserData,
        pub status: UserDataOutsiderStatus,
    }

    impl UserDataOutsider {
        pub fn unknown(user_data: UserData) -> Self {
            Self {
                user_data,
                status: UserDataOutsiderStatus::Unknown,
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum UserDataOutsiderStatus {
        /// Unknown status (the user is not a member of the vault)
        Unknown,
        Pending,
        Declined,
    }

    impl UserMembership {
        pub fn user_data(&self) -> UserData {
            match self {
                UserMembership::Outsider(outsider) => {
                    outsider.user_data().clone()
                }
                UserMembership::Member(member) => {
                    member.user_data.clone()
                }
            }
        }

        pub fn device_id(&self) -> DeviceId {
            self.user_data().device.id.clone()
        }
    }
}

pub mod vault {
    use std::collections::{HashMap, HashSet};
    use std::fmt::Display;

    use crate::node::common::model::device::{DeviceData, DeviceId};
    use crate::node::common::model::MetaPasswordId;
    use crate::node::common::model::user::{UserData, UserDataMember, UserDataOutsider, UserMembership};
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::vault_event::VaultObject;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VaultName(pub String);

    impl From<String> for VaultName {
        fn from(vault_name: String) -> Self {
            Self(vault_name)
        }
    }

    impl From<&str> for VaultName {
        fn from(vault_name: &str) -> Self {
            VaultName::from(String::from(vault_name))
        }
    }

    impl Display for VaultName {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0.clone())
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VaultData {
        pub vault_name: VaultName,
        pub users: HashMap<DeviceId, UserMembership>,
        pub secrets: HashSet<MetaPasswordId>,
    }

    impl From<VaultName> for VaultData {
        fn from(vault_name: VaultName) -> Self {
            VaultData {
                vault_name,
                users: HashMap::new(),
                secrets: HashSet::new(),
            }
        }
    }

    impl VaultData {
        pub fn members(&self) -> Vec<UserDataMember> {
            let mut members: Vec<UserDataMember> = vec![];
            self.users.values().for_each(|membership| {
                if let UserMembership::Member(user_data_member) = membership {
                    members.push(user_data_member.clone());
                }
            });

            members
        }

        pub fn add_secret(&mut self, meta_password_id: MetaPasswordId) {
            self.secrets.insert(meta_password_id);
        }

        pub fn update_membership(&mut self, membership: UserMembership) {
            self.users.insert(membership.device_id(), membership);
        }

        pub fn is_member(&self, device: &DeviceData) -> bool {
            let maybe_user = self.users.get(&device.id);
            if let Some(UserMembership::Member(UserDataMember { user_data })) = maybe_user {
                user_data.device == device.clone()
            } else {
                false
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum VaultStatus {
        Outsider(UserDataOutsider),
        Member(VaultData),
    }

    impl VaultStatus {
        pub fn try_from(vault_event: GenericKvLogEvent, user: UserData) -> anyhow::Result<Self> {
            let vault_obj = VaultObject::try_from(vault_event)?;
            match vault_obj {
                VaultObject::Unit { .. } => {
                    Ok(VaultStatus::Outsider(UserDataOutsider::unknown(user)))
                }
                VaultObject::Genesis { .. } => {
                    Ok(VaultStatus::Outsider(UserDataOutsider::unknown(user)))
                }
                VaultObject::Vault { event } => {
                    Ok(VaultStatus::Member(event.value))
                }
            }
        }
    }

    impl VaultStatus {
        pub fn unknown(user: UserData) -> Self {
            VaultStatus::Outsider(UserDataOutsider::unknown(user))
        }
    }
}

pub mod crypto {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::node::common::model::device::{DeviceData, DeviceId, DeviceLink};
    use crate::node::common::model::user::UserData;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadAuthData {
        pub associated_data: String,
        pub channel: CommunicationChannel,
        pub nonce: Base64Text,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadCipherText {
        pub msg: Base64Text,
        pub auth_data: AeadAuthData,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AeadPlainText {
        pub msg: Base64Text,
        pub auth_data: AeadAuthData,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CommunicationChannel {
        pub sender: Base64Text,
        pub receiver: Base64Text,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum EncryptedMessage {
        /// There is only one type of encrypted message for now, which is encrypted share of a secret,
        /// and that particular type of message has a device link
        /// and it used to figure out which vault the message belongs to
        CipherShare {
            device_link: DeviceLink,
            share: AeadCipherText
        }
    }

    impl EncryptedMessage {
        pub fn device_link(&self) -> DeviceLink {
            match self {
                EncryptedMessage::CipherShare { device_link, .. } => device_link.clone()
            }
        }
    }
}

pub mod secret {
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    use crate::crypto::utils;
    use crate::node::common::model::crypto::EncryptedMessage;
    use crate::node::common::model::device::DeviceLink;
    use crate::node::common::model::vault::VaultName;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MetaPasswordId {
        /// SHA256 hash of a salt
        pub id: String,
        /// Random String up to 30 characters, must be unique
        pub salt: String,
        /// Human readable name given to the password
        pub name: String,
    }

    const SALT_LENGTH: usize = 8;

    impl MetaPasswordId {
        pub fn generate(name: String) -> Self {
            let salt: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(SALT_LENGTH)
                .map(char::from)
                .collect();
            MetaPasswordId::build(name, salt)
        }

        pub fn build(name: String, salt: String) -> Self {
            let mut id_str = name.clone();
            id_str.push('-');
            id_str.push_str(salt.as_str());

            Self {
                id: utils::generate_uuid_b64_url_enc(id_str),
                salt,
                name,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum SecretDistributionType {
        Split,
        Recover,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SecretDistributionData {
        pub distribution_type: SecretDistributionType,
        pub vault_name: VaultName,
        pub meta_password_id: MetaPasswordId,
        pub secret_message: EncryptedMessage,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PasswordRecoveryRequest {
        pub id: MetaPasswordId,
        pub device_link: DeviceLink
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationStatus {
    Registered,
    AlreadyExists,
}

impl Display for RegistrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Self::Registered => String::from("Registered"),
            Self::AlreadyExists => String::from("AlreadyExists"),
        };
        write!(f, "{}", str)
    }
}

impl Default for RegistrationStatus {
    fn default() -> RegistrationStatus {
        Self::Registered
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationState {
    pub device: Option<DeviceData>,
    pub vault: Option<VaultStatus>,
    pub join_component: bool,
}

impl Default for ApplicationState {
    fn default() -> Self {
        ApplicationState {
            device: None,
            vault: None,
            join_component: false,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn meta_password_id() {
        let pass_id = MetaPasswordId::build("test".to_string(), "salt".to_string());
        assert_eq!(pass_id.id, "CHKANX39xaMXfhe3Qkx9-w".to_string())
    }
}
