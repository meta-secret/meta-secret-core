use crate::node::common::model::user::common::{UserDataMember, UserMembership};
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultMembershipDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::anyhow;
use crate::node::common::model::vault::vault::VaultStatus;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatusObject(pub KvLogEvent<VaultStatus>);

impl VaultStatusObject {
    pub fn new(membership: UserMembership, event_id: ArtifactId) -> Self {
        let user_id = membership.user_data().user_id();
        let desc = VaultMembershipDescriptor::from(user_id).to_obj_desc();

        VaultStatusObject(KvLogEvent {
            key: KvKey {
                obj_id: event_id,
                obj_desc: desc,
            },
            value: membership,
        })
    }
    
    pub fn membership(self) -> UserMembership {
        self.0.value
    }
}

impl VaultStatusObject {
    pub fn is_member(&self) -> bool {
        let VaultStatusObject(membership_event) = self;

        matches!(
            membership_event.value,
            UserMembership::Member(UserDataMember { .. })
        )
    }

    pub fn is_not_member(&self) -> bool {
        !self.is_member()
    }
}

impl TryFrom<GenericKvLogEvent> for VaultStatusObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::VaultMembership(vault_status) = event {
            Ok(vault_status)
        } else {
            Err(anyhow!("Not a vault status event"))
        }
    }
}

impl KeyExtractor for VaultStatusObject {
    fn key(&self) -> KvKey {
        self.0.key.clone()
    }
}

impl ToGenericEvent for VaultStatusObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::VaultMembership(self)
    }
}

impl ObjIdExtractor for VaultStatusObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}
