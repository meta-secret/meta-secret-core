use crate::node::common::model::MetaPasswordId;
use crate::node::common::model::user::UserData;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericAppStateRequest {
    SignUp { user: UserData},
    Recover { meta_pass_id: MetaPasswordId},
    ClusterDistribution(ClusterDistributionRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClusterDistributionRequest {
    pub pass_id: String,
    pub pass: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {}