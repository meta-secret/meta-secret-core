//! Fixed local keys; no public traversal or mutation capability is introduced.
use super::object_descriptor::{ObjectDescriptor, ObjectName, ObjectType, ToObjectDescriptor};
use crate::node::{
    common::model::user::common::UserId,
    db::events::local_guarded_preparation::{
        LocalGuardedPreparationIndexObject, LocalGuardedPreparationObject,
    },
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedPreparationIndexDescriptor {
    pub owner: UserId,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGuardedPreparationDescriptor {
    pub owner: UserId,
    pub request_id: String,
}

impl std::fmt::Debug for LocalGuardedPreparationIndexDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalGuardedPreparationIndexDescriptor")
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for LocalGuardedPreparationDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalGuardedPreparationDescriptor")
            .finish_non_exhaustive()
    }
}

impl ToObjectDescriptor for LocalGuardedPreparationIndexDescriptor {
    type EventType = LocalGuardedPreparationIndexObject;
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::LocalGuardedPreparationIndex(self)
    }
}
impl ObjectType for LocalGuardedPreparationIndexDescriptor {
    fn object_type(&self) -> String {
        "LocalGuardedPreparationIndex".into()
    }
}
impl ObjectName for LocalGuardedPreparationIndexDescriptor {
    fn object_name(&self) -> String {
        owner_name(&self.owner)
    }
}

impl ToObjectDescriptor for LocalGuardedPreparationDescriptor {
    type EventType = LocalGuardedPreparationObject;
    fn to_obj_desc(self) -> ObjectDescriptor {
        ObjectDescriptor::LocalGuardedPreparation(self)
    }
}
impl ObjectType for LocalGuardedPreparationDescriptor {
    fn object_type(&self) -> String {
        "LocalGuardedPreparation".into()
    }
}
impl ObjectName for LocalGuardedPreparationDescriptor {
    fn object_name(&self) -> String {
        format!("{}{}", owner_name(&self.owner), part(&self.request_id))
    }
}

fn owner_name(owner: &UserId) -> String {
    format!(
        "{}{}",
        part(&owner.vault_name.0),
        part(&owner.device_id.to_string())
    )
}
fn part(value: &str) -> String {
    format!("{}:{value}", value.len())
}
