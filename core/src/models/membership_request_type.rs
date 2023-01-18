/*
 * Meta Secret Core Models
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 1.0.0
 * 
 * Generated by: https://openapi-generator.tech
 */


/// 
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum MembershipRequestType {
    #[serde(rename = "Accept")]
    Accept,
    #[serde(rename = "Decline")]
    Decline,

}

impl ToString for MembershipRequestType {
    fn to_string(&self) -> String {
        match self {
            Self::Accept => String::from("Accept"),
            Self::Decline => String::from("Decline"),
        }
    }
}

impl Default for MembershipRequestType {
    fn default() -> MembershipRequestType {
        Self::Accept
    }
}



