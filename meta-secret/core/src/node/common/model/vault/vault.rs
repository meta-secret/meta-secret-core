use crate::crypto::utils::Id48bit;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::secret::{
    SecretDistributionType, SsDistributionClaim, SsDistributionClaimId, SsLogData, WasmSsLogData,
};
use crate::node::common::model::user::common::{UserData, UserDataMember, UserDataOutsider};
use crate::node::common::model::vault::vault_data::{VaultData, WasmVaultData};
use std::fmt::Display;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[wasm_bindgen]
impl VaultName {
    pub fn generate() -> Self {
        let id_str = Id48bit::generate().text;
        Self(id_str)
    }

    pub fn test() -> VaultName {
        VaultName::from("q")
    }
}

/////////////////// VaultStatus ///////////////////
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatus {
    NotExists(UserData),
    Outsider(UserDataOutsider),
    Member(UserDataMember)
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
        matches!(&self.0, VaultStatus::Member { .. })
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
            VaultStatus::Member { member, ss_claims } => {
                WasmVaultMember(member.clone(), WasmSsLogData::from(ss_claims.clone()))
            }
            _ => panic!("Vault status is not 'member'"),
        }
    }
}

impl From<VaultStatus> for WasmVaultStatus {
    fn from(status: VaultStatus) -> Self {
        Self(status)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMember {
    pub member: UserDataMember,
    pub vault: VaultData,
}

impl VaultMember {
    pub fn create_split_claim(&self, pass_id: MetaPasswordId) -> SsDistributionClaim {
        self.create_distribution_claim(pass_id, SecretDistributionType::Split)
    }

    pub fn create_recovery_claim(&self, pass_id: MetaPasswordId) -> SsDistributionClaim {
        self.create_distribution_claim(pass_id, SecretDistributionType::Recover)
    }

    fn create_distribution_claim(
        &self,
        pass_id: MetaPasswordId,
        distribution_type: SecretDistributionType,
    ) -> SsDistributionClaim {
        let links = self
            .vault
            .members()
            .iter()
            .filter_map(|vault_member| {
                if vault_member.eq(&self.member) {
                    None
                } else {
                    Some(vault_member.user_data.device.device_id.clone())
                }
            })
            .collect();

        SsDistributionClaim {
            id: SsDistributionClaimId::from(pass_id),
            vault_name: self.vault.vault_name.clone(),
            sender: self.user_device(),
            distribution_type,
            receivers: links,
        }
    }

    fn user_device(&self) -> DeviceId {
        self.member.user().device.device_id.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct WasmVaultMember(VaultMember, WasmSsLogData);
#[wasm_bindgen]
impl WasmVaultMember {
    pub fn vault_data(&self) -> WasmVaultData {
        WasmVaultData::from(self.0.vault.clone())
    }
}

impl From<(VaultMember, WasmSsLogData)> for WasmVaultMember {
    fn from(member_and_ss: (VaultMember, WasmSsLogData)) -> Self {
        Self(member_and_ss.0, member_and_ss.1)
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
            VaultStatus::Member { member, .. } => member.member.user().clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::UserMembership;
    use anyhow::Result;

    #[test]
    fn test_vault_status() -> Result<()> {
        let fixture = FixtureRegistry::empty();

        let client_creds = fixture.state.user_creds.client;
        let client_membership = UserMembership::Member(UserDataMember {
            user_data: client_creds.user(),
        });

        let vd_creds = fixture.state.user_creds.vd;
        let vd_membership = UserMembership::Member(UserDataMember {
            user_data: vd_creds.user(),
        });

        let vault_data =
            VaultData::from(client_creds.user()).update_membership(vd_membership.clone());

        let vault_member = VaultMember {
            member: client_membership.user_data_member(),
            vault: vault_data,
        };

        let pass_id = MetaPasswordId::generate(String::from("test_password"));
        let claim = vault_member.create_split_claim(pass_id);
        assert_eq!(1, claim.receivers.len());

        Ok(())
    }
}
