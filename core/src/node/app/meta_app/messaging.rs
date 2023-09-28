use crate::models::MetaPasswordId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "app_state_request")]
pub enum GenericAppStateRequest {
    SignUp(SignUpRequest),
    Recover(RecoveryRequest),
    ClusterDistribution(ClusterDistributionRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignUpRequest {
    pub vault_name: String,
    pub device_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub meta_pass_id: MetaPasswordId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterDistributionRequest {
    pub pass_id: String,
    pub pass: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {}
