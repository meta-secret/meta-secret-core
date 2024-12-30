use crate::node::common::model::meta_pass::MetaPasswordId;
use crate::node::common::model::vault::vault::VaultName;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericAppStateRequest {
    SignUp(VaultName),
    ClusterDistribution(ClusterDistributionRequest),
    Recover(MetaPasswordId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterDistributionRequest {
    pub pass_id: MetaPasswordId,
    pub pass: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {}
