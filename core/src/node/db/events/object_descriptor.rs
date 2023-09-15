use crate::crypto::utils;
use crate::models::SecretDistributionDocData;

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

    /// Secret distribution (split, recover, recovery request and so on)
    SharedSecret {
        vault_name: String,
        device_id: String,
    },

    MetaVault,
    UserCreds,
}

impl From<&SecretDistributionDocData> for ObjectDescriptor {
    fn from(value: &SecretDistributionDocData) -> Self {
        let vault_name = value.meta_password.meta_password.vault.vault_name.clone();
        let device_id = value.secret_message.receiver.vault.device.device_id.clone();
        ObjectDescriptor::SharedSecret { vault_name, device_id }
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

            ObjectDescriptor::SharedSecret { vault_name, device_id } => {
                let mut name = vault_name.clone();
                name.push('-');
                name.push_str(device_id.as_str());

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
            ObjectDescriptor::SharedSecret { .. } => String::from("SharedSecret"),

            ObjectDescriptor::MetaPassword { .. } => String::from("MetaPass"),
            ObjectDescriptor::LocalSecretShare { .. } => String::from("LocalSecretShare"),

            ObjectDescriptor::MetaVault { .. } => String::from("MetaVault"),
            ObjectDescriptor::UserCreds { .. } => String::from("UserCreds"),
            ObjectDescriptor::DbTail { .. } => String::from("DbTail"),
        }
    }
}
