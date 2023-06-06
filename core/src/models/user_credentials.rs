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
pub struct UserCredentials {
    #[serde(rename = "securityBox")]
    pub security_box: Box<crate::models::UserSecurityBox>,
    #[serde(rename = "userSig")]
    pub user_sig: Box<crate::models::UserSignature>,
}

impl UserCredentials {
    pub fn new(security_box: crate::models::UserSecurityBox, user_sig: crate::models::UserSignature) -> UserCredentials {
        UserCredentials {
            security_box: Box::new(security_box),
            user_sig: Box::new(user_sig),
        }
    }
}


