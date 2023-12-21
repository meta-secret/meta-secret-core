use crate::node::common::model::secret::MetaPasswordId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericAppStateRequest {
    SignUp,
    ClusterDistribution(ClusterDistributionRequest),
    Recover(MetaPasswordId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterDistributionRequest {
    pub pass_id: String,
    pub pass: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {}