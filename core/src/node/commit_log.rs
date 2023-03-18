use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEvent {
    pub key: Key,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub ns: String,
    pub store: String,
    pub id: String,
    pub event_type: EventType
}

pub enum EventType {
    SignUp
}