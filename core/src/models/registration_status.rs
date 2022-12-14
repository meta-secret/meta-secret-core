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
pub enum RegistrationStatus {
    #[serde(rename = "Registered")]
    Registered,
    #[serde(rename = "AlreadyExists")]
    AlreadyExists,

}

impl ToString for RegistrationStatus {
    fn to_string(&self) -> String {
        match self {
            Self::Registered => String::from("Registered"),
            Self::AlreadyExists => String::from("AlreadyExists"),
        }
    }
}

impl Default for RegistrationStatus {
    fn default() -> RegistrationStatus {
        Self::Registered
    }
}




