use crate::models::MetaPasswordId;
use crate::node::common::model::device::DeviceData;
use crate::node::common::model::vault::VaultName;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "__app_state_request")]
pub enum GenericAppStateRequest {
    SignUp(SignUpRequest),
    Recover(RecoveryRequest),
    ClusterDistribution(ClusterDistributionRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignUpRequest {
    pub vault_name: VaultName,
    pub device_name: DeviceData,
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
