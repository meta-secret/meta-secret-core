use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::secret::SsLogData;
use crate::node::common::model::user::common::{UserData, UserDataOutsider};
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::common::model::vault::vault_data::WasmVaultData;
use crate::node::db::events::vault::vault_log_event::VaultActionEvents;
use wasm_bindgen::prelude::wasm_bindgen;

pub mod crypto;
pub mod device;
pub mod meta_pass;
pub mod secret;
pub mod user;
pub mod vault;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub enum  ApplicationStateInfo {
    Local,
    Member,
    Outsider,
    VaultNotExists,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationState {
    Local(DeviceData),
    Vault(VaultFullInfo),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultFullInfo {
    NotExists(UserData),
    Outsider(UserDataOutsider),
    Member(UserMemberFullInfo),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemberFullInfo {
    pub member: VaultMember,
    pub ss_claims: SsLogData,
    pub vault_events: VaultActionEvents,
}

#[wasm_bindgen]
pub struct WasmVaultFullInfo(VaultFullInfo);

#[wasm_bindgen]
pub struct WasmUserMemberFullInfo(UserMemberFullInfo);

#[wasm_bindgen]
pub struct WasmApplicationState(ApplicationState);

#[wasm_bindgen]
impl WasmApplicationState {

    pub fn as_info(&self) -> ApplicationStateInfo {
        match &self.0 {
            ApplicationState::Local(_) => ApplicationStateInfo::Local,
            ApplicationState::Vault(vault_info) => {
                match vault_info {
                    VaultFullInfo::NotExists(_) => ApplicationStateInfo::VaultNotExists,
                    VaultFullInfo::Outsider(_) => ApplicationStateInfo::Outsider,
                    VaultFullInfo::Member(_) => ApplicationStateInfo::Member
                }
            }
        }
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

    pub fn is_outsider(&self) -> bool {
        matches!(self.0, VaultFullInfo::Outsider(_))
    }

    pub fn is_vault_not_exists(&self) -> bool {
        matches!(self.0, VaultFullInfo::NotExists(_))
    }

    pub fn as_outsider(&self) -> UserDataOutsider {
        if let VaultFullInfo::Outsider(outsider) = &self.0 {
            outsider.clone()
        } else {
            panic!("not an outsider")
        }
    }

    pub fn as_not_exists(&self) -> UserData {
        if let VaultFullInfo::NotExists(user_data) = &self.0 {
            user_data.clone()
        } else {
            panic!("Is not a vault not exists")
        }
    }

    pub fn as_member(&self) -> WasmUserMemberFullInfo {
        if let VaultFullInfo::Member(member) = &self.0 {
            WasmUserMemberFullInfo(member.clone())
        } else {
            panic!("not a member vault info")
        }
    }
    
    pub fn vault_name(&self) -> String {
        let vault_name = match &self.0 {
            VaultFullInfo::Member(member) => &member.member.vault.vault_name,
            VaultFullInfo::Outsider(outsider) => &outsider.user_data.vault_name,
            VaultFullInfo::NotExists(user_data) => &user_data.vault_name,
        };
        
        vault_name.0.clone()
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
    use crate::node::common::model::IdString;
    use crate::node::common::model::meta_pass::MetaPasswordId;

    #[test]
    fn meta_password_id() {
        let pass_id = MetaPasswordId::build_from_str("test");
        assert_eq!(pass_id.id.id_str(), "n4bQgYhMfWU".to_string())
    }
}
