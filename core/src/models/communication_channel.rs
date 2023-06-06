/*
 * Meta Secret Core Models
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 1.0.0
 * 
 * Generated by: https://openapi-generator.tech
 */

/// CommunicationChannel : Represents virtual encrypted communication channel between two points.



#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct CommunicationChannel {
    #[serde(rename = "sender")]
    pub sender: Box<crate::models::Base64EncodedText>,
    #[serde(rename = "receiver")]
    pub receiver: Box<crate::models::Base64EncodedText>,
}

impl CommunicationChannel {
    /// Represents virtual encrypted communication channel between two points.
    pub fn new(sender: crate::models::Base64EncodedText, receiver: crate::models::Base64EncodedText) -> CommunicationChannel {
        CommunicationChannel {
            sender: Box::new(sender),
            receiver: Box::new(receiver),
        }
    }
}


