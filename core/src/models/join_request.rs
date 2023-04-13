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
pub struct JoinRequest {
    #[serde(rename = "member")]
    pub member: Box<crate::models::UserSignature>,
    #[serde(rename = "candidate")]
    pub candidate: Box<crate::models::UserSignature>,
}

impl JoinRequest {
    pub fn new(member: crate::models::UserSignature, candidate: crate::models::UserSignature) -> JoinRequest {
        JoinRequest {
            member: Box::new(member),
            candidate: Box::new(candidate),
        }
    }
}
