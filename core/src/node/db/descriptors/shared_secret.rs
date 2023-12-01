use crate::node::common::model::{MetaPasswordId, SecretDistributionData, SecretDistributionType};
use crate::node::common::model::device::DeviceId;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectType};

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

    /// This log allows to recreate a lifetime of the secret sharing workflow and allows to have a consistent view
    ///across the cluster on what events of the secret sharing happened at what time.
    SSLog {
        vault_name: VaultName,
    },
}

impl ObjectType for SharedSecretDescriptor {
    fn object_type(&self) -> String {
        match self {
            SharedSecretDescriptor::Split(_) => String::from("SSSplit"),
            SharedSecretDescriptor::Recover(_) => String::from("SSRecover"),
            SharedSecretDescriptor::SSLog { .. } => String::from("SSLog"),
            SharedSecretDescriptor::LocalShare => String::from("SSShare")
        }
    }
}

impl SharedSecretDescriptor {
    pub fn as_id_str(&self) -> String {
        match self {
            SharedSecretDescriptor::Split(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::Recover(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::SSLog { vault_name } => vault_name.to_string(),
            SharedSecretDescriptor::LocalShare { .. } => {
                serde_json::to_string(self).unwrap()
            }
        }
    }

    pub fn audit(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(SharedSecretDescriptor::SSLog { vault_name })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSecretEventId {
    pub vault_name: VaultName,
    pub sender: DeviceId,
    pub receiver: DeviceId,
}

impl SharedSecretEventId {
    pub fn as_id_str(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

impl From<&SecretDistributionData> for ObjectDescriptor {
    fn from(secret_distribution: &SecretDistributionData) -> Self {
        let vault_name = secret_distribution.vault_name.clone();
        let receiver_device = secret_distribution.secret_message.receiver_device();
        let sender_device = secret_distribution.secret_message.sender_device();

        let ss_event_id = SharedSecretEventId {
            vault_name,
            sender: sender_device.id.clone(),
            receiver: receiver_device.id,
        };
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

#[cfg(test)]
mod test {
    use crate::crypto::keys::{KeyManager, OpenBox, SecretBox};
    use crate::node::common::model::device::DeviceId;
    use crate::node::common::model::MetaPasswordId;
    use crate::node::common::model::vault::VaultName;
    use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
    use crate::node::db::descriptors::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};

    #[test]
    fn test_shared_secret_split() {
        let secret_box = {
            let key_manager = KeyManager::generate();
            SecretBox::from(key_manager)
        };

        let device_id = {
            let open_box = OpenBox::from(&secret_box);
            DeviceId::from(&open_box)
        };

        let obj_desc = {
            let event_id = SharedSecretEventId {
                vault_name: VaultName(String::from("test_vault")),
                sender: device_id.clone(),
                receiver: device_id,
            };
            ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split(event_id))
        };

        let expected = format!("SSSplit:test_vault-{}::0", device_id.to_string());
        assert_eq!(expected, obj_desc.to_fqdn());
    }
}