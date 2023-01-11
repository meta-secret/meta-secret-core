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
pub struct MetaPasswordRequest {
    #[serde(rename = "userSig")]
    pub user_sig: Box<crate::models::UserSignature>,
    #[serde(rename = "metaPassword")]
    pub meta_password: Box<crate::models::MetaPasswordDoc>,
}

impl MetaPasswordRequest {
    pub fn new(user_sig: crate::models::UserSignature, meta_password: crate::models::MetaPasswordDoc) -> MetaPasswordRequest {
        MetaPasswordRequest {
            user_sig: Box::new(user_sig),
            meta_password: Box::new(meta_password),
        }
    }
}

