use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::secret::SsLogData;
use crate::node::common::model::user::common::{UserData, UserDataOutsider};
use crate::node::common::model::vault::vault::VaultMember;
use wasm_bindgen::prelude::wasm_bindgen;
use crate::node::common::model::vault::vault_data::{VaultData, WasmVaultData};

pub mod crypto;
pub mod device;
pub mod meta_pass;
pub mod secret;
pub mod user;
pub mod vault;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationState {
    Local(DeviceData),
    Vault(VaultFullInfo),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultFullInfo {
    NotExists(UserData),
    Outsider(UserDataOutsider),
    Member(UserMemberFullInfo),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemberFullInfo {
    pub member: VaultMember,
    pub ss_claims: SsLogData,
}

#[wasm_bindgen]
pub struct WasmVaultFullInfo(VaultFullInfo);

#[wasm_bindgen]
pub struct WasmUserMemberFullInfo(UserMemberFullInfo);

#[wasm_bindgen]
pub struct WasmApplicationState(ApplicationState);

#[wasm_bindgen]
impl WasmApplicationState {
    pub fn is_new_user(&self) -> bool {
        let is_local = self.is_local();
        let vault_not_exists =
            matches!(&self.0, ApplicationState::Vault(VaultFullInfo::NotExists(_)));
        is_local || vault_not_exists
    }

    pub fn is_local(&self) -> bool {
        matches!(self.0, ApplicationState::Local { .. })
    }

    pub fn is_vault(&self) -> bool {
        matches!(self.0, ApplicationState::Vault { .. })
    }

    pub fn as_local(&self) -> DeviceData {
        if let ApplicationState::Local(device) = &self.0 {
            device.clone()
        } else {
            panic!("not a local app state")
        }
    }

    pub fn as_vault(&self) -> WasmVaultFullInfo {
        let ApplicationState::Vault(full_info) = &self.0 else {
            panic!("not a vault app state");
        };

        WasmVaultFullInfo(full_info.clone())
    }
}

impl From<ApplicationState> for WasmApplicationState {
    fn from(state: ApplicationState) -> Self {
        WasmApplicationState(state)
    }
}

pub trait IdString {
    fn id_str(self) -> String;
}

#[wasm_bindgen]
impl WasmVaultFullInfo {
    pub fn is_member(&self) -> bool {
        matches!(self.0, VaultFullInfo::Member(_))
    }
    
    pub fn as_member(&self) -> WasmUserMemberFullInfo {
        if let VaultFullInfo::Member(member) = &self.0 {
            WasmUserMemberFullInfo(member.clone())
        } else {
            panic!("not a member vault info")
        }
    }
}

#[wasm_bindgen]
impl WasmUserMemberFullInfo {
    pub fn vault_data(&self) -> WasmVaultData {
        WasmVaultData::from(self.0.member.vault.clone())
    }
}

#[cfg(test)]
mod test {
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::IdString;

    #[test]
    fn meta_password_id() {
        let pass_id = MetaPasswordId::build("test");
        assert_eq!(pass_id.id.id_str(), "n4bQgYhMfWU".to_string())
    }
}
