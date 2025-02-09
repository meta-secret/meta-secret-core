use crate::node::common::model::user::common::{UserDataMember, UserMembership};
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultMembershipDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::anyhow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMembershipObject(KvLogEvent<UserMembership>);

impl VaultMembershipObject {
    pub fn new(membership: UserMembership, event_id: ArtifactId) -> Self {
        let user_id = membership.user_data().user_id();
        let desc = VaultMembershipDescriptor::from(user_id).to_obj_desc();

        VaultMembershipObject(KvLogEvent {
            key: KvKey {
                obj_id: event_id,
                obj_desc: desc,
            },
            value: membership,
        })
    }
}

impl VaultMembershipObject {
    pub fn is_member(&self) -> bool {
        let VaultMembershipObject(membership_event) = self;

        matches!(
            membership_event.value,
            UserMembership::Member(UserDataMember { .. })
        )
    }

    pub fn is_not_member(&self) -> bool {
        !self.is_member()
    }
}

impl TryFrom<GenericKvLogEvent> for VaultMembershipObject {
    type Error = anyhow::Error;

    fn try_from(event: GenericKvLogEvent) -> Result<Self, Self::Error> {
        if let GenericKvLogEvent::VaultMembership(vault_status) = event {
            Ok(vault_status)
        } else {
            Err(anyhow!("Not a vault status event"))
        }
    }
}

impl KeyExtractor for VaultMembershipObject {
    fn key(&self) -> KvKey {
       self.0.key.clone()
    }
}

impl ToGenericEvent for VaultMembershipObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::VaultMembership(self)
    }
}

impl ObjIdExtractor for VaultMembershipObject {
    fn obj_id(&self) -> ArtifactId {
        self.0.key.obj_id.clone()
    }
}
