use crate::node::common::model::IdString;
use crate::node::db::descriptors::creds::CredentialsDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::{
    SsWorkflowDescriptor, SsDeviceLogDescriptor, SsLogDescriptor,
};
use crate::node::db::descriptors::vault_descriptor::{
    DeviceLogDescriptor, VaultDescriptor, VaultLogDescriptor, VaultStatusDescriptor,
};
use crate::node::db::events::generic_log_event::GenericKvLogEventConvertible;
use crate::node::db::events::object_id::Next;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectDescriptor {
    /// Describes device and user credentials
    Creds(CredentialsDescriptor),

    DeviceLog(DeviceLogDescriptor),

    VaultLog(VaultLogDescriptor),
    Vault(VaultDescriptor),
    VaultStatus(VaultStatusDescriptor),

    /// Secret distribution (split, recover, recovery request and so on)
    SsLog(SsLogDescriptor),
    SsDeviceLog(SsDeviceLogDescriptor),
    SharedSecret(SsWorkflowDescriptor),
}

pub trait ToObjectDescriptor: Clone {
    type EventType: GenericKvLogEventConvertible;
    fn to_obj_desc(self) -> ObjectDescriptor;
}

pub trait ObjectType {
    fn object_type(&self) -> String;
}

pub trait ObjectName {
    fn object_name(&self) -> String;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectFqdn {
    pub obj_type: String,
    pub obj_instance: String,
}

impl IdString for ObjectFqdn {
    fn id_str(self) -> String {
        format!("{}:{}", self.obj_type, self.obj_instance)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChainId {
    Genesis(GenesisId),
    Seq(SeqId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenesisId;

impl Next<SeqId> for GenesisId {
    fn next(self) -> SeqId {
        let curr = 0;
        SeqId {
            curr: curr + 1,
            prev: curr,
        }
    }
}

/// Sequential Id
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeqId {
    pub curr: usize,
    prev: usize,
}

impl SeqId {
    pub fn first() -> Self {
        GenesisId.next()
    }
}

impl Next<SeqId> for SeqId {
    fn next(mut self) -> SeqId {
        self.prev += 1;
        self.curr += 1;
        self
    }
}

impl ObjectDescriptor {
    /// Fully Qualified Domain Name - unique domain name of an object
    pub fn fqdn(&self) -> ObjectFqdn {
        ObjectFqdn {
            obj_type: self.object_type(),
            obj_instance: self.object_name(),
        }
    }
}

impl ObjectName for ObjectDescriptor {
    fn object_name(&self) -> String {
        match self {
            ObjectDescriptor::Creds(desc) => desc.object_name(),

            ObjectDescriptor::Vault(vault_desc) => vault_desc.object_name(),
            ObjectDescriptor::DeviceLog(device_log) => device_log.object_name(),
            ObjectDescriptor::VaultLog(vault_log) => vault_log.object_name(),
            ObjectDescriptor::VaultStatus(membership) => membership.object_name(),

            ObjectDescriptor::SharedSecret(s_s_descriptor) => s_s_descriptor.clone().id_str(),
            ObjectDescriptor::SsLog(desc) => desc.clone().id_str(),
            ObjectDescriptor::SsDeviceLog(desc) => desc.clone().id_str(),
        }
    }
}

impl ObjectType for ObjectDescriptor {
    fn object_type(&self) -> String {
        match self {
            ObjectDescriptor::Vault(vault_desc) => vault_desc.object_type(),
            ObjectDescriptor::SharedSecret(ss_desc) => ss_desc.object_type(),
            ObjectDescriptor::Creds(creds) => creds.object_type(),
            ObjectDescriptor::DeviceLog(device_log) => device_log.object_type(),
            ObjectDescriptor::VaultLog(vault_log) => vault_log.object_type(),
            ObjectDescriptor::VaultStatus(mem) => mem.object_type(),
            ObjectDescriptor::SsLog(desc) => desc.object_type(),
            ObjectDescriptor::SsDeviceLog(desc) => desc.object_type(),
        }
    }
}

#[cfg(test)]
mod seq_id_tests {
    use super::*;

    #[test]
    fn test_seq_id_first() {
        let first_id = SeqId::first();
        assert_eq!(first_id.curr, 1);
        assert_eq!(first_id.prev, 0);
    }

    #[test]
    fn test_seq_id_next() {
        let first_id = SeqId::first();
        let next_id = first_id.next();
        
        assert_eq!(next_id.curr, 2);
        assert_eq!(next_id.prev, 1);
        
        // Test multiple next calls
        let third_id = next_id.next();
        assert_eq!(third_id.curr, 3);
        assert_eq!(third_id.prev, 2);
    }

    #[test]
    fn test_genesis_to_seq_id() {
        let genesis = GenesisId;
        let seq_id = genesis.next();
        
        assert_eq!(seq_id.curr, 1);
        assert_eq!(seq_id.prev, 0);
    }
}

#[cfg(test)]
mod fqdn_tests {
    use crate::node::common::model::IdString;
    use crate::node::common::model::vault::vault::VaultName;
    use crate::node::db::descriptors::creds::CredentialsDescriptor;
    use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;

    #[test]
    fn test_object_fqdn_id_string() {
        // Test with CredentialsDescriptor
        {
            let creds_device = CredentialsDescriptor::Device;
            let creds_descriptor = creds_device.to_obj_desc();
            let creds_fqdn = creds_descriptor.fqdn();
            assert_eq!(creds_fqdn.id_str(), "DeviceCreds:index");
        }

        {
            // Test with VaultDescriptor 
            let vault_name = VaultName::from("test_vault");
            let vault_descriptor = VaultDescriptor::from(vault_name).to_obj_desc();
            let vault_fqdn = vault_descriptor.fqdn();
            assert_eq!(vault_fqdn.id_str(), "Vault:test_vault");
        }

        {
            let creds_device = CredentialsDescriptor::Device;
            let creds_descriptor = creds_device.to_obj_desc();
            let creds_fqdn = creds_descriptor.fqdn();
            assert_eq!(creds_fqdn.id_str(), "DeviceCreds:index");
        }
    }
}