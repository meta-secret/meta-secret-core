use crate::node::common::model::crypto::CommunicationChannel;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::device::device_link::DeviceLink;
use crate::node::common::model::secret::{MetaPasswordId, SecretDistributionType, SsDistributionClaim, SsDistributionClaimId};
use crate::node::common::model::user::common::{
    UserData, UserDataMember, UserDataOutsider, UserMembership, WasmUserMembership,
};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::secret::data_block::common::SharedSecretConfig;
use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(getter_with_clone)]
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
    pub fn test() -> VaultName {
        VaultName::from("q")
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

#[wasm_bindgen(getter_with_clone)]
pub struct WasmVaultData(VaultData);
#[wasm_bindgen]
impl WasmVaultData {
    pub fn vault_name(&self) -> VaultName {
        self.0.vault_name.clone()
    }

    pub fn users(&self) -> Vec<WasmUserMembership> {
        self.0
            .users
            .values()
            .map(|m| WasmUserMembership::from(m.clone()))
            .collect()
    }

    pub fn members(&self) -> Vec<UserDataMember> {
        self.0.members()
    }

    pub fn outsiders(&self) -> Vec<UserDataOutsider> {
        self.0.outsiders()
    }

    pub fn secrets(&self) -> Vec<MetaPasswordId> {
        self.0.secrets.iter().map(|pass| pass.clone()).collect()
    }
}

impl From<VaultData> for WasmVaultData {
    fn from(vault_data: VaultData) -> Self {
        WasmVaultData(vault_data)
    }
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
    pub fn sss_cfg(&self) -> SharedSecretConfig {
        let members_num = self.members().len();
        SharedSecretConfig::calculate(members_num)
    }

    pub fn members(&self) -> Vec<UserDataMember> {
        let mut members: Vec<UserDataMember> = vec![];
        self.users.values().for_each(|membership| {
            if let UserMembership::Member(user_data_member) = membership {
                members.push(user_data_member.clone());
            }
        });

        members
    }

    pub fn outsiders(&self) -> Vec<UserDataOutsider> {
        let mut outsiders: Vec<UserDataOutsider> = vec![];
        self.users.values().for_each(|membership| {
            if let UserMembership::Outsider(outsider) = membership {
                outsiders.push(outsider.clone());
            }
        });

        outsiders
    }

    pub fn add_secret(&mut self, meta_password_id: MetaPasswordId) {
        self.secrets.insert(meta_password_id);
    }

    pub fn update_membership(&self, membership: UserMembership) -> Self {
        let mut new_vault = self.clone();
        new_vault.users.insert(membership.device_id(), membership);
        new_vault
    }

    pub fn is_not_member(&self, device_id: &DeviceId) -> bool {
        !self.is_member(device_id)
    }

    pub fn is_member(&self, device_id: &DeviceId) -> bool {
        let maybe_user = self.users.get(device_id);
        if let Some(UserMembership::Member(UserDataMember { user_data })) = maybe_user {
            user_data.device.device_id.eq(&device_id)
        } else {
            false
        }
    }

    pub fn membership(&self, for_user: UserData) -> UserMembership {
        let maybe_vault_user = self.users.get(&for_user.device.device_id);

        if let Some(membership) = maybe_vault_user {
            membership.clone()
        } else {
            UserMembership::Outsider(UserDataOutsider::non_member(for_user))
        }
    }

    pub fn find_user(&self, device_id: &DeviceId) -> Option<UserMembership> {
        self.users.get(device_id).map(|user| user.clone())
    }

    pub fn build_communication_channel(&self, device_link: DeviceLink) -> Option<CommunicationChannel> {
        match device_link {
            DeviceLink::Loopback(loopback_link) => {
                let sender = loopback_link.device();
                let maybe_sender = self.find_user(sender);

                match maybe_sender {
                    Some(UserMembership::Member(UserDataMember { user_data })) => {
                        let pk = user_data.device.keys.transport_pk;
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

                match (maybe_sender, maybe_receiver) {
                    (
                        Some(UserMembership::Member(UserDataMember { user_data: sender })),
                        Some(UserMembership::Member(UserDataMember { user_data: receiver }))
                    ) => {
                        let sender_pk = sender.device.keys.transport_pk.clone();
                        let receiver_pk = receiver.device.keys.transport_pk.clone();

                        let channel = CommunicationChannel {
                            sender: sender_pk,
                            receiver: receiver_pk,
                        };
                        Some(channel)
                    }
                    (_, _) => {
                        None
                    }
                }
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
    Member(VaultMember),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct WasmVaultStatus(VaultStatus);
#[wasm_bindgen]
impl WasmVaultStatus {
    pub fn is_not_exists(&self) -> bool {
        matches!(&self.0, VaultStatus::NotExists(_))
    }

    pub fn is_outsider(&self) -> bool {
        matches!(&self.0, VaultStatus::Outsider(_))
    }

    pub fn is_member(&self) -> bool {
        matches!(&self.0, VaultStatus::Member(_))
    }

    pub fn as_no_exists(&self) -> UserData {
        match &self.0 {
            VaultStatus::NotExists(user_data) => user_data.clone(),
            _ => panic!("Vault status is not 'not exists'"),
        }
    }

    pub fn as_outsider(&self) -> UserDataOutsider {
        match &self.0 {
            VaultStatus::Outsider(outsider) => outsider.clone(),
            _ => panic!("Vault status is not 'outsider'"),
        }
    }

    pub fn as_member(&self) -> WasmVaultMember {
        match &self.0 {
            VaultStatus::Member(member) => WasmVaultMember(member.clone()),
            _ => panic!("Vault status is not 'member'"),
        }
    }
}

impl From<VaultStatus> for WasmVaultStatus {
    fn from(status: VaultStatus) -> Self {
        WasmVaultStatus(status)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMember {
    pub member: UserDataMember,
    pub vault: VaultData,
}

impl VaultMember {
    pub fn create_split_claim(&self, pass_id: MetaPasswordId) -> Result<SsDistributionClaim> {
        self.create_distribution_claim(pass_id, SecretDistributionType::Split)
    }

    pub fn create_recover_claim(&self, pass_id: MetaPasswordId) -> Result<SsDistributionClaim> {
        self.create_distribution_claim(pass_id, SecretDistributionType::Recover)
    }
    
    fn create_distribution_claim(
        &self, pass_id: MetaPasswordId, distribution_type: SecretDistributionType
    ) -> Result<SsDistributionClaim> {
        let mut distributions = vec![];
        for vault_member in self.vault.members() {
            if vault_member == self.member {
                continue;
            }

            let link = {
                let receiver_device = vault_member.user_data.device.device_id.clone();
                self.user_device().make_device_link(receiver_device)?
            };

            distributions.push(link);
        }

        Ok(SsDistributionClaim {
            vault_name: self.vault.vault_name.clone(),
            owner: self.user_device(),
            id: SsDistributionClaimId::generate(),
            pass_id,
            distribution_type,
            distributions,
        })
    }

    fn user_device(&self) -> DeviceId {
        self.member.user().device.device_id.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct WasmVaultMember(VaultMember);
#[wasm_bindgen]
impl WasmVaultMember {
    pub fn vault_data(&self) -> WasmVaultData {
        WasmVaultData::from(self.0.vault.clone())
    }
}

impl From<VaultMember> for WasmVaultMember {
    fn from(vault_member: VaultMember) -> Self {
        Self(vault_member)
    }
}

impl VaultStatus {
    pub fn unknown(user: UserData) -> Self {
        VaultStatus::Outsider(UserDataOutsider::non_member(user))
    }

    pub fn user(&self) -> UserData {
        match self {
            VaultStatus::NotExists(user) => user.clone(),
            VaultStatus::Outsider(UserDataOutsider { user_data, .. }) => user_data.clone(),
            VaultStatus::Member(VaultMember { member, .. }) => member.user().clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use super::*;
    #[test]
    fn test_vault_status() -> Result<()> {
        let fixture = FixtureRegistry::empty();
        
        let client_creds = fixture.state.user_creds.client;
        let client_user_data = UserDataMember { user_data: client_creds.user() };
        let client_membership = UserMembership::Member(client_user_data.clone());

        let vd_creds = fixture.state.user_creds.vd;
        let vd_user_data = UserDataMember { user_data: vd_creds.user() };
        let vd_membership = UserMembership::Member(vd_user_data.clone());
        
        let mut vault_data = VaultData::from(client_creds.vault_name.clone());
        vault_data = vault_data
            .update_membership(client_membership)
            .update_membership(vd_membership);

        let vault_member = VaultMember {
            member: client_user_data.clone(),
            vault: vault_data,
        };

        let pass_id = MetaPasswordId::generate(String::from("test_password"));
        let claim = vault_member.create_split_claim(pass_id)?;
        assert_eq!(1, claim.distributions.len());
        
        Ok(())
    }
}