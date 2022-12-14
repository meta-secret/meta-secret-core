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
pub struct AeadAuthData {
    #[serde(rename = "associatedData")]
    pub associated_data: String,
    #[serde(rename = "channel")]
    pub channel: Box<crate::models::CommunicationChannel>,
    #[serde(rename = "nonce")]
    pub nonce: Box<crate::models::Base64EncodedText>,
}

impl AeadAuthData {
    pub fn new(associated_data: String, channel: crate::models::CommunicationChannel, nonce: crate::models::Base64EncodedText) -> AeadAuthData {
        AeadAuthData {
            associated_data,
            channel: Box::new(channel),
            nonce: Box::new(nonce),
        }
    }
}


