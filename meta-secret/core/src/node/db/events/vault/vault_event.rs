use crate::node::common::model::user::common::UserDataMember;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::vault::vault_data::VaultData;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent, VaultKvLogEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::anyhow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultObject(pub KvLogEvent<VaultData>);

impl VaultObject {
    pub fn sign_up(vault_name: VaultName, candidate: UserDataMember) -> Self {
        let desc = VaultDescriptor::from(vault_name.clone());
        let vault_data = VaultData::from(candidate);

        let sign_up_event = KvLogEvent {
            key: KvKey::from(desc),
            value: vault_data,
        };
        VaultObject(sign_up_event)
    }

    pub fn to_data(self) -> VaultData {
        self.0.value
    }
}

impl TryFrom<GenericKvLogEvent> for VaultObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::Vault(VaultKvLogEvent::Vault(vault)) = event {
            Ok(vault)
        } else {
            Err(anyhow!("Not a vault event"))
        }
    }
}

impl ToGenericEvent for VaultObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::Vault(VaultKvLogEvent::Vault(self))
    }
}

impl ObjIdExtractor for VaultObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}

impl KeyExtractor for VaultObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;

    #[test]
    fn test_vault_object_sign_up() {
        let fixture = FixtureRegistry::empty();
        let user_creds = fixture.state.user_creds.client;

        // Assuming VaultObject::sign_up creates a new event for the vault sign up
        let sign_up =
            VaultObject::sign_up(VaultName::test(), UserDataMember::from(user_creds.user()));

        assert_eq!(1, sign_up.0.value.users.len());
    }
}
