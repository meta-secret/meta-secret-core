use crate::node::common::model::crypto::EncryptedMessage;
use crate::node::common::model::device::{DeviceId, DeviceLink};
use crate::node::common::model::secret::{MetaPasswordId, SecretDistributionData, SecretDistributionType};
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectType, ToObjectDescriptor};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretDescriptor {
    /// Local share of a secret
    LocalShare {
        vault_name: VaultName,
        meta_pass_id: MetaPasswordId,
    },

    Split(SharedSecretEventId),
    Recover(SharedSecretEventId),

    SSDeviceLog(DeviceId),

    /// This log allows to recreate a lifetime of the secret sharing workflow and allows to have a consistent view
    ///across the cluster on what events of the secret sharing happened at what time.
    SSLog(VaultName)
}

impl ObjectType for SharedSecretDescriptor {
    fn object_type(&self) -> String {
        match self {
            SharedSecretDescriptor::Split(_) => String::from("SSSplit"),
            SharedSecretDescriptor::Recover(_) => String::from("SSRecover"),
            SharedSecretDescriptor::SSLog { .. } => String::from("SSLog"),
            SharedSecretDescriptor::LocalShare { .. } => String::from("SSLocalShare"),
            SharedSecretDescriptor::SSDeviceLog(_) => String::from("SSDeviceLog")
        }
    }
}

impl SharedSecretDescriptor {
    pub fn as_id_str(&self) -> String {
        match self {
            SharedSecretDescriptor::Split(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::Recover(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::SSLog(vault_name) => vault_name.to_string(),
            SharedSecretDescriptor::LocalShare { .. } => {
                serde_json::to_string(self).unwrap()
            }
            SharedSecretDescriptor::SSDeviceLog(device_id) => device_id.to_string()
        }
    }

    pub fn audit(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(SharedSecretDescriptor::SSLog(vault_name))
    }
}

impl ToObjectDescriptor for SharedSecretDescriptor {
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSecretEventId {
    pub vault_name: VaultName,
    pub device_link: DeviceLink
}

impl SharedSecretEventId {
    pub fn as_id_str(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

impl From<SecretDistributionData> for SharedSecretEventId {
    fn from(secret: SecretDistributionData) -> Self {
        let device_link = match secret.secret_message {
            EncryptedMessage::CipherShare { device_link, .. } => device_link
        };

        Self { vault_name: secret.vault_name, device_link }
    }
}

impl From<&SecretDistributionData> for ObjectDescriptor {
    fn from(secret_distribution: &SecretDistributionData) -> Self {

        let ss_event_id = SharedSecretEventId::from(secret_distribution.clone());

        match secret_distribution.distribution_type {
            SecretDistributionType::Split => {
                ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split(ss_event_id))
            }
            SecretDistributionType::Recover => {
                ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Recover(ss_event_id))
            }
        }
    }
}
