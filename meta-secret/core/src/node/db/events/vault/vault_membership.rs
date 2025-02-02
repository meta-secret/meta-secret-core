use crate::node::common::model::user::common::{UserData, UserDataMember, UserMembership};
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultMembershipDescriptor;
use crate::node::db::events::generic_log_event::{
    GenericKvLogEvent, KeyExtractor, ObjIdExtractor, ToGenericEvent,
};
use crate::node::db::events::kv_log_event::{GenericKvKey, KvKey, KvLogEvent};
use crate::node::db::events::object_id::{
    ArtifactId, ObjectId, VaultGenesisEvent, VaultUnitEvent,
};
use anyhow::anyhow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultMembershipObject {
    Unit(VaultUnitEvent),
    /// Device public keys
    Genesis(VaultGenesisEvent),
    Membership(KvLogEvent<ArtifactId, UserMembership>),
}

impl VaultMembershipObject {
    pub fn init(candidate: UserData) -> Vec<GenericKvLogEvent> {
        let unit_event = VaultMembershipObject::unit(candidate.clone()).to_generic();
        let genesis_event = VaultMembershipObject::genesis(candidate.clone()).to_generic();

        let member_event = {
            let desc = VaultMembershipDescriptor::from(candidate.user_id());
            let member_event_id = ArtifactId::from(desc);
            VaultMembershipObject::member(candidate, member_event_id).to_generic()
        };

        vec![unit_event, genesis_event, member_event]
    }

    fn unit(candidate: UserData) -> Self {
        let user_id = candidate.user_id();
        let desc = VaultMembershipDescriptor::from(user_id);

        VaultMembershipObject::Unit(VaultUnitEvent(KvLogEvent {
            key: KvKey::unit_from(desc),
            value: candidate.vault_name,
        }))
    }

    pub fn genesis(candidate: UserData) -> Self {
        let desc = VaultMembershipDescriptor::from(candidate.user_id());

        VaultMembershipObject::Genesis(VaultGenesisEvent(KvLogEvent {
            key: KvKey::genesis(desc.to_obj_desc()),
            value: candidate.clone(),
        }))
    }

    pub fn member(candidate: UserData, event_id: ArtifactId) -> Self {
        let member = UserMembership::Member(UserDataMember {
            user_data: candidate.clone(),
        });
        Self::membership(member, event_id)
    }

    pub fn membership(membership: UserMembership, event_id: ArtifactId) -> Self {
        let user_id = membership.user_data().user_id();
        let desc = VaultMembershipDescriptor::from(user_id).to_obj_desc();

        VaultMembershipObject::Membership(KvLogEvent {
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
        let VaultMembershipObject::Membership(membership_event) = self else {
            return false;
        };

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
    fn key(&self) -> GenericKvKey {
        match self {
            VaultMembershipObject::Unit(event) => GenericKvKey::from(event.key()),
            VaultMembershipObject::Genesis(event) => GenericKvKey::from(event.key()),
            VaultMembershipObject::Membership(event) => GenericKvKey::from(event.key.clone()),
        }
    }
}

impl ToGenericEvent for VaultMembershipObject {
    fn to_generic(self) -> GenericKvLogEvent {
        GenericKvLogEvent::VaultMembership(self)
    }
}

impl ObjIdExtractor for VaultMembershipObject {
    fn obj_id(&self) -> ObjectId {
        match self {
            VaultMembershipObject::Unit(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultMembershipObject::Genesis(event) => ObjectId::from(event.key().obj_id.clone()),
            VaultMembershipObject::Membership(event) => ObjectId::from(event.key.obj_id.clone()),
        }
    }
}
