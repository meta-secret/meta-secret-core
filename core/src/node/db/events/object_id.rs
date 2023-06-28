use serde_derive::{Deserialize, Serialize};

use crate::node::db::models::{ObjectCreator, ObjectDescriptor};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectId {
    Genesis {
        id: String
    },
    Regular {
        id: String,
        genesis_id: String,
    },
}

impl ObjectId {
    pub fn genesis_id(&self) -> Self {
        match self {
            ObjectId::Genesis { .. } => {
                self.clone()
            }
            ObjectId::Regular { genesis_id, .. } => {
                Self::Genesis {
                    id: genesis_id.clone(),
                }
            }
        }
    }

    pub fn id_str(&self) -> String {
        match self {
            ObjectId::Genesis { id } => { id.clone() }
            ObjectId::Regular { id, .. } => { id.clone() }
        }
    }
}

impl ObjectCreator<&ObjectDescriptor> for ObjectId {
    fn formation(obj_descriptor: &ObjectDescriptor) -> Self {
        let genesis_id = obj_descriptor.to_id();
        Self::Genesis { id: genesis_id }
    }
}


#[cfg(test)]
mod test {
    use crate::node::db::events::object_id::ObjectId;

    #[test]
    fn json_parsing_test() {
        let obj_id = ObjectId::Genesis { id: "test".to_string() };
        let obj_id_json = serde_json::to_string(&obj_id).unwrap();
        println!("{}", obj_id_json);
    }
}