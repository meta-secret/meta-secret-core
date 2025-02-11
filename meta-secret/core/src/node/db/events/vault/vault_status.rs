use crate::node::common::model::user::common::UserDataMember;
use crate::node::common::model::vault::vault::VaultStatus;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultStatusDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::ArtifactId;
use anyhow::anyhow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatusObject(pub KvLogEvent<VaultStatus>);

impl VaultStatusObject {
    pub fn new(status: VaultStatus, event_id: ArtifactId) -> Self {
        let user_id = status.user().user_id();
        let desc = VaultStatusDescriptor::from(user_id).to_obj_desc();

        VaultStatusObject(KvLogEvent {
            key: KvKey {
                obj_id: event_id,
                obj_desc: desc,
            },
            value: status,
        })
    }
    
    pub fn status(self) -> VaultStatus {
        self.0.value
    }
}

impl VaultStatusObject {
    pub fn is_member(&self) -> bool {
        let VaultStatusObject(membership_event) = self;

        matches!(
            membership_event.value,
            VaultStatus::Member(UserDataMember { .. })
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
