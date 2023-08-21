use crate::crypto::utils;
use crate::models::{DeviceInfo, SecretDistributionDocData, SecretDistributionType};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    GlobalIndex,
    Mempool,
    DbTail,
    Vault {
        vault_name: String,
    },

    MetaPassword {
        vault_name: String,
    },

    LocalSecretShare {
        meta_pass_id: String,
    },
    SecretShareDistribution {
        meta_pass_id: String,
        action_type: SecretDistributionType,
        device: DeviceInfo,
    },

    MetaVault,
    UserCreds,
}

impl From<&SecretDistributionDocData> for ObjectDescriptor {
    fn from(value: &SecretDistributionDocData) -> Self {
        ObjectDescriptor::SecretShareDistribution {
            meta_pass_id: value.meta_password.meta_password.id.id.clone(),
            action_type: value.distribution_type,
            device: value.secret_message.receiver.vault.device.as_ref().clone(),
        }
    }
}

impl ObjectDescriptor {
    pub fn to_id(&self) -> String {
        utils::to_id(self.full_name().as_str())
    }

    pub fn vault(vault_name: String) -> ObjectDescriptor {
        ObjectDescriptor::Vault { vault_name }
    }
}

impl ObjectDescriptor {
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.to_string(), self.name())
    }

    pub fn name(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex => String::from("index"),
            ObjectDescriptor::Mempool => String::from("mem_pool"),

            ObjectDescriptor::DbTail => String::from("db_tail"),
            ObjectDescriptor::Vault { vault_name } => vault_name.clone(),

            ObjectDescriptor::SecretShareDistribution {
                meta_pass_id,
                action_type,
                device,
            } => {
                let mut name = meta_pass_id.clone();
                name.push_str(action_type.to_string().as_str());
                name.push_str(device.device_id.as_str());

                name
            }

            ObjectDescriptor::MetaPassword { vault_name } => vault_name.clone(),
            ObjectDescriptor::LocalSecretShare { meta_pass_id } => meta_pass_id.clone(),

            ObjectDescriptor::MetaVault => String::from("index"),
            ObjectDescriptor::UserCreds => String::from("index"),
        }
    }
}

impl ToString for ObjectDescriptor {
    fn to_string(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex { .. } => String::from("GlobalIndex"),
            ObjectDescriptor::Mempool { .. } => String::from("Mempool"),

            ObjectDescriptor::Vault { .. } => String::from("Vault"),
            ObjectDescriptor::SecretShareDistribution { .. } => String::from("ShareDist"),

            ObjectDescriptor::MetaPassword { .. } => String::from("MetaPass"),
            ObjectDescriptor::LocalSecretShare { .. } => String::from("SecretShare"),

            ObjectDescriptor::MetaVault { .. } => String::from("MetaVault"),
            ObjectDescriptor::UserCreds { .. } => String::from("UserCreds"),
            ObjectDescriptor::DbTail { .. } => String::from("DbTail"),
        }
    }
}
