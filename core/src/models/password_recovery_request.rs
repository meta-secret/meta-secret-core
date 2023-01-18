/*
 * Meta Secret Core Models
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 1.0.0
 * 
 * Generated by: https://openapi-generator.tech
 */




#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct PasswordRecoveryRequest {
    #[serde(rename = "id")]
    pub id: Box<crate::models::MetaPasswordId>,
    #[serde(rename = "consumer")]
    pub consumer: Box<crate::models::UserSignature>,
    #[serde(rename = "provider")]
    pub provider: Box<crate::models::UserSignature>,
}

impl PasswordRecoveryRequest {
    pub fn new(id: crate::models::MetaPasswordId, consumer: crate::models::UserSignature, provider: crate::models::UserSignature) -> PasswordRecoveryRequest {
        PasswordRecoveryRequest {
            id: Box::new(id),
            consumer: Box::new(consumer),
            provider: Box::new(provider),
        }
    }
}

