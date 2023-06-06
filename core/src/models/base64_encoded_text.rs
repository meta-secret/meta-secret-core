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
pub struct Base64EncodedText {
    #[serde(rename = "base64Text")]
    pub base64_text: String,
}

impl Base64EncodedText {
    pub fn new(base64_text: String) -> Base64EncodedText {
        Base64EncodedText {
            base64_text,
        }
    }
}


