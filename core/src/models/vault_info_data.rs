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
pub struct VaultInfoData {
    #[serde(rename = "vaultInfo", skip_serializing_if = "Option::is_none")]
    pub vault_info: Option<crate::models::VaultInfoStatus>,
    #[serde(rename = "vault", skip_serializing_if = "Option::is_none")]
    pub vault: Option<Box<crate::models::VaultDoc>>,
}

impl VaultInfoData {
    pub fn new() -> VaultInfoData {
        VaultInfoData {
            vault_info: None,
            vault: None,
        }
    }
}


