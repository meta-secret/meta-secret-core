use crate::node::common::model::meta_pass::{MetaPasswordId, PassInfo};
use crate::node::common::model::vault::vault::VaultName;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericAppStateRequest {
    SignUp(VaultName),
    ClusterDistribution(PassInfo),
    Recover(MetaPasswordId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {}
