use crate::node::common::model::{MetaPasswordId, SecretDistributionDocData, SecretDistributionType};
use crate::node::common::model::device::DeviceId;
use crate::node::common::model::vault::VaultName;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ObjectType};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedSecretDescriptor {
    Split(SharedSecretEventId),
    Recover(SharedSecretEventId),
    RecoveryRequest(SharedSecretEventId),

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
    Audit {
        vault_name: VaultName,
    },
}

impl ObjectType for SharedSecretDescriptor {
    fn object_type(&self) -> String {
        match self {
            SharedSecretDescriptor::Split(_) => String::from("SSSplit"),
            SharedSecretDescriptor::Recover(_) => String::from("SSRecover"),
            SharedSecretDescriptor::RecoveryRequest(_) => String::from("SSRecoveryRequest"),
            SharedSecretDescriptor::Audit { .. } => String::from("SSAudit")
        }
    }
}

impl SharedSecretDescriptor {
    pub fn as_id_str(&self) -> String {
        match self {
            SharedSecretDescriptor::Split(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::Recover(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::RecoveryRequest(event_id) => event_id.as_id_str(),
            SharedSecretDescriptor::Audit { vault_name } => vault_name.0.clone(),
        }
    }

    pub fn audit(vault_name: VaultName) -> ObjectDescriptor {
        ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Audit { vault_name })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSecretEventId {
    pub vault_name: VaultName,
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
        let key_manager = KeyManager::generate();
        let secret_box = SecretBox::from(key_manager);
        let device_id = DeviceId::from(&OpenBox::from(&secret_box));

        let event_id = SharedSecretEventId {
            vault_name: VaultName(String::from("test_vault")),
            meta_pass_id: MetaPasswordId::build(String::from("test_meta_pass"), String::from("salt")),
            receiver: device_id,
        };

        let obj_desc = ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split(event_id));
        let expected = format!("SSSplit:test_vault-{}::0", device_id.to_string());
        assert_eq!(expected, obj_desc.to_fqdn())
    }
}