use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;
use crate::node::db::events::object_descriptor::shared_secret::SharedSecretDescriptor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__obj_desc")]
pub enum ObjectDescriptor {
    GlobalIndex(GlobalIndexDescriptor),

    MemPool,
    DbTail,

    Vault {
        vault_name: String,
    },

    MetaPassword {
        vault_name: String,
    },

    /// Secret distribution (split, recover, recovery request and so on)
    SharedSecret(SharedSecretDescriptor),

    /// This Audit log allows to recreate a lifetime of the secret sharing workflow and allows to have a consistent view
    ///across the cluster on what events of the secret sharing happened at what time.
    /// All the nodes of the system can use this log to sync data (split, recover events) exactly once and keep track
    /// of secret shares across the cluster and sync/replicate those shares efficiently between nodes and have more deterministic
    /// flow of actions and events for the secret sharing mechanism.
    ///
    /// We will add new events into the audit log which will indicate that the event has been happened.
    /// For instance:
    /// we want to send a secret share from one device to another device.
    ///  - the device_a creates a new SharedSecretAuditDescriptor::SplitEvent and puts it into the audit table (which contains the object_id of the split event).
    ///  - the device_a creates a SharedSecretDescriptor::Split event (which contains an actual secret share (data))
    ///  - sync gateway sends the audit event to the server, and sends the split event also to the server
    ///  - when device_b syncs the data, the server looks into the audit table, analyses tail records, takes all split/recover events to be sent to the device.
    ///
    /// By looking into the audit log (since the audit contains the information about what secret shares were created and sent)
    /// we know what split/recover events needs to be sent synchronized
    SharedSecretAudit {
        vault_name: String,
    },

    DeviceCredsIndex,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDescriptorFqdn {
    pub obj_type: String,
    pub obj_instance: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDescriptorId {
    pub fqdn: ObjectDescriptorFqdn,
    pub counter: usize,
}

impl ObjectDescriptor {
    pub fn to_fqdn(&self) -> ObjectDescriptorFqdn {
        self.fqdn()
    }

    pub fn vault(vault_name: String) -> ObjectDescriptor {
        ObjectDescriptor::Vault { vault_name }
    }
}

impl ObjectDescriptor {
    /// Fully Qualified Domain Name - unique domain name of an object
    pub fn fqdn(&self) -> ObjectDescriptorFqdn {
        ObjectDescriptorFqdn {
            obj_type: self.object_type(),
            obj_instance: self.object_name(),
        }
    }

    pub fn object_name(&self) -> String {
        match self {
            ObjectDescriptor::MemPool => String::from("mem_pool"),

            ObjectDescriptor::DbTail => String::from("db_tail"),
            ObjectDescriptor::Vault { vault_name } => vault_name.clone(),

            ObjectDescriptor::SharedSecret(s_s_descriptor) => s_s_descriptor.as_id_str(),
            ObjectDescriptor::SharedSecretAudit { vault_name } => vault_name.clone(),

            ObjectDescriptor::MetaPassword { vault_name } => vault_name.clone(),
            ObjectDescriptor::DeviceCredsIndex => String::from("index"),
            ObjectDescriptor::GlobalIndex(desc) => desc.as_id_str()
        }
    }
}

impl ToString for ObjectDescriptor {
    fn to_string(&self) -> String {
        self.object_type()
    }
}

pub trait ObjectType {
    fn object_type(&self) -> String;
}

impl ObjectType for ObjectDescriptor {
    fn object_type(&self) -> String {
        match self {
            ObjectDescriptor::GlobalIndex(gi_desc) => gi_desc.object_type(),
            ObjectDescriptor::MemPool { .. } => String::from("MemPool"),

            ObjectDescriptor::Vault { .. } => String::from("Vault"),
            ObjectDescriptor::SharedSecret(ss_desc) => ss_desc.object_type(),

            ObjectDescriptor::SharedSecretAudit { .. } => String::from("SSAudit"),

            ObjectDescriptor::MetaPassword { .. } => String::from("MetaPass"),
            ObjectDescriptor::DeviceCredsIndex { .. } => String::from("UserCreds"),
            ObjectDescriptor::DbTail { .. } => String::from("DbTail"),
        }
    }
}

pub mod global_index {
    use crate::crypto::utils;
    use crate::node::db::events::object_descriptor::ObjectType;
    use crate::node::db::events::object_id::UnitId;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum GlobalIndexDescriptor {
        Index,
        /// An id of a vault. We have global index to keep track and being able to iterate over all vaults,
        /// and to be able to check if a particular vault exists we ned to have vault index
        VaultIndex { vault_id: UnitId },
    }

    impl ObjectType for GlobalIndexDescriptor {
        fn object_type(&self) -> String {
            match self {
                GlobalIndexDescriptor::Index => String::from("GlobalIndex"),
                GlobalIndexDescriptor::VaultIndex { .. } => String::from("VaultIdx")
            }
        }
    }

    impl GlobalIndexDescriptor {
        pub fn as_id_str(&self) -> String {
            match self {
                GlobalIndexDescriptor::Index => String::from("index"),
                GlobalIndexDescriptor::VaultIndex { vault_id } => {
                    let json_str = serde_json::to_string(&vault_id.id).unwrap();
                    utils::generate_uuid_b64_url_enc(json_str)
                }
            }
        }
    }
}

pub mod shared_secret {
    use crate::models::{MetaPasswordId, SecretDistributionDocData, SecretDistributionType};
    use crate::node::common::model::device::DeviceId;
    use crate::node::db::events::object_descriptor::{ObjectDescriptor, ObjectType};

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum SharedSecretDescriptor {
        Split(SharedSecretEventId),
        Recover(SharedSecretEventId),
        RecoveryRequest(SharedSecretEventId),
    }

    impl ObjectType for SharedSecretDescriptor {
        fn object_type(&self) -> String {
            match self {
                SharedSecretDescriptor::Split(_) => String::from("SSSplit"),
                SharedSecretDescriptor::Recover(_) => String::from("SSRecover"),
                SharedSecretDescriptor::RecoveryRequest(_) => String::from("SSRecoveryRequest"),
            }
        }
    }

    impl SharedSecretDescriptor {
        pub fn as_id_str(&self) -> String {
            match self {
                SharedSecretDescriptor::Split(event_id) => event_id.as_id_str(),
                SharedSecretDescriptor::Recover(event_id) => event_id.as_id_str(),
                SharedSecretDescriptor::RecoveryRequest(event_id) => event_id.as_id_str(),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SharedSecretEventId {
        pub vault_name: String,
        pub meta_pass_id: MetaPasswordId,
        pub receiver: DeviceId,
    }

    impl SharedSecretEventId {
        pub fn as_id_str(&self) -> String {
            let pattern = [
                self.vault_name.as_str(),
                self.meta_pass_id.id.as_str(),
                self.receiver.to_string().as_str(),
            ];
            pattern.join("-")
        }
    }

    impl From<&SecretDistributionDocData> for ObjectDescriptor {
        fn from(secret_distribution: &SecretDistributionDocData) -> Self {
            let vault_name = secret_distribution.meta_password.meta_password.vault.vault_name.clone();
            let device_id = secret_distribution
                .secret_message
                .receiver
                .vault
                .device
                .as_ref()
                .clone();

            let meta_pass_id = secret_distribution.meta_password.meta_password.id.as_ref().clone();
            let ss_event_id = SharedSecretEventId {
                vault_name,
                meta_pass_id,
                receiver: device_id,
            };
            match secret_distribution.distribution_type {
                SecretDistributionType::Split => {
                    ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split(ss_event_id))
                },
                SecretDistributionType::Recover => {
                    ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Recover(ss_event_id))
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::keys::{KeyManager, OpenBox, SecretBox};
    use crate::models::MetaPasswordId;
    use crate::node::common::model::device::DeviceId;
    use crate::node::db::events::object_descriptor::global_index::GlobalIndexDescriptor;
    use crate::node::db::events::object_descriptor::ObjectDescriptor;
    use crate::node::db::events::object_descriptor::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};

    #[test]
    fn test_global_index() {
        let obj_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index);
        assert_eq!(String::from("GlobalIndex:index::0"), obj_desc.to_fqdn())
    }

    #[test]
    fn test_vault() {
        let obj_desc = ObjectDescriptor::Vault {
            vault_name: String::from("test"),
        };
        assert_eq!(String::from("Vault:test::0"), obj_desc.to_fqdn())
    }

    #[test]
    fn test_meta_pass() {
        let obj_desc = ObjectDescriptor::MetaPassword {
            vault_name: String::from("test"),
        };
        assert_eq!(String::from("MetaPass:test::0"), obj_desc.to_fqdn())
    }

    #[test]
    fn test_shared_secret_split() {
        let key_manager = KeyManager::generate();
        let secret_box = SecretBox::from(key_manager);
        let device_id = DeviceId::from(&OpenBox::from(&secret_box));

        let event_id = SharedSecretEventId {
            vault_name: String::from("test_vault"),
            meta_pass_id: MetaPasswordId::build(String::from("test_meta_pass"), String::from("salt")),
            receiver: device_id,
        };

        let obj_desc = ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split(event_id));
        let expected = format!("SSSplit:test_vault-{}::0", device_id.to_string());
        assert_eq!(expected, obj_desc.to_fqdn())
    }
}
