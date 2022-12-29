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
pub struct SerializedTransportKeyPair {
    #[serde(rename = "secretKey")]
    pub secret_key: Box<crate::models::Base64EncodedText>,
    #[serde(rename = "publicKey")]
    pub public_key: Box<crate::models::Base64EncodedText>,
}

impl SerializedTransportKeyPair {
    pub fn new(secret_key: crate::models::Base64EncodedText, public_key: crate::models::Base64EncodedText) -> SerializedTransportKeyPair {
        SerializedTransportKeyPair {
            secret_key: Box::new(secret_key),
            public_key: Box::new(public_key),
        }
    }
}


