use crate::models::{MetaPasswordDoc, VaultDoc};
use crate::node::common::model::device::DeviceCredentials;

pub mod device {
    use crate::crypto;
    use crate::crypto::keys::OpenBox;
    use crate::crypto::keys::SecretBox;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceData {
        id: DeviceId,
        name: String,
        pub keys: OpenBox
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceId(String);

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DeviceCredentials {
        pub secret_box: SecretBox,
        pub device: DeviceData
    }

    impl DeviceData {
        pub fn from(device_name: String, open_box: OpenBox) -> Self {
            Self {
                name: device_name,
                id: DeviceId::from(&open_box),
                keys: open_box,
            }
        }
    }

    impl From<&OpenBox> for DeviceId {
        fn from(open_box: &OpenBox) -> Self {
            Self(crypto::utils::generate_uuid_b64_url_enc(open_box.dsa_pk.base64_text.clone()))
        }
    }
}

pub mod vault {
    use crate::node::common::model::device::DeviceData;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UserId {
        pub vault_id: VaultId,
        pub device: DeviceData,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VaultId {
        id: String,
        name: String
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationState {
    pub device_creds: Option<DeviceCredentials>,
    pub vault: Option<VaultDoc>,
    pub meta_passwords: Vec<MetaPasswordDoc>,
    pub join_component: bool,
}
