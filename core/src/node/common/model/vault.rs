use crate::node::common::model::MetaPasswordId;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::device::device_link::DeviceLink;
use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider, UserMembership};
use super::crypto::CommunicationChannel;

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

impl VaultName {
    pub fn client() -> Self {
        VaultName::from("client")
    }

    pub fn vd() -> Self {
        VaultName::from("vd")
    }

    pub fn server() -> Self {
        VaultName::from("server")
    }
}

/////////////////// VaultData ///////////////////
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

    pub fn is_member(&self, device_id: &DeviceId) -> bool {
        let maybe_user = self.users.get(device_id);
        if let Some(UserMembership::Member(UserDataMember(user_data))) = maybe_user {
            user_data.device.id == device_id.clone()
        } else {
            false
        }
    }

    pub fn status(&self, for_user: UserData) -> VaultStatus {
        let maybe_vault_user = self.users.get(&for_user.device.id);

        match maybe_vault_user {
            Some(vault_user) => match vault_user {
                UserMembership::Outsider(outsider) => VaultStatus::Outsider(outsider.clone()),
                UserMembership::Member(member) => VaultStatus::Member {
                    member: member.clone(),
                    vault: self.clone(),
                },
            },
            None => VaultStatus::Outsider(UserDataOutsider::non_member(for_user)),
        }
    }

    pub fn find_user(&self, device_id: &DeviceId) -> Option<UserMembership> {
        self.users.get(device_id).map(|user| user.clone())
    }

    pub fn build_communication_channel(&self, device_link: DeviceLink) -> Option<CommunicationChannel> {
        match device_link {
            DeviceLink::Loopback(loopback_link) => {
                let sender = loopback_link.device();
                let maybe_user = self.find_user(sender);

                match maybe_user {
                    Some(UserMembership::Member(UserDataMember(user))) => {
                        let pk = user.device.keys.transport_pk;
                        let channel = CommunicationChannel {
                            sender: pk.clone(),
                            receiver: pk.clone(),
                        };
                        Some(channel)
                    }
                    _ => None,
                }
            }
            DeviceLink::PeerToPeer(p2p_link) => {
                let sender_device = p2p_link.sender();
                let receiver_device = p2p_link.receiver();

                let maybe_sender = self.find_user(sender_device);
                let maybe_receiver = self.find_user(receiver_device);

                let Some(UserMembership::Member(UserDataMember(sender))) = maybe_sender else {
                    return None;
                };

                let Some(UserMembership::Member(UserDataMember(receiver))) = maybe_receiver else {
                    return None;
                };

                let sender_pk = sender.device.keys.transport_pk.clone();
                let receiver_pk = receiver.device.keys.transport_pk.clone();

                let channel = CommunicationChannel {
                    sender: sender_pk,
                    receiver: receiver_pk,
                };
                Some(channel)
            }
        }
    }
}

/////////////////// VaultStatus ///////////////////
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatus {
    NotExists(UserData),
    Outsider(UserDataOutsider),
    Member { member: UserDataMember, vault: VaultData },
}

impl VaultStatus {
    pub fn unknown(user: UserData) -> Self {
        VaultStatus::Outsider(UserDataOutsider::non_member(user))
    }

    pub fn user(&self) -> UserData {
        match self {
            VaultStatus::NotExists(user) => user.clone(),
            VaultStatus::Outsider(UserDataOutsider { user_data, .. }) => user_data.clone(),
            VaultStatus::Member { member, .. } => member.user().clone(),
        }
    }
}
