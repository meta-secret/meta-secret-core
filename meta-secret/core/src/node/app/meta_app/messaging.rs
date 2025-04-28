use crate::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use crate::node::common::model::vault::vault::VaultName;

#[derive(Clone, Debug, PartialEq)]
pub enum GenericAppStateRequest {
    SignUp(VaultName),
    ClusterDistribution(PlainPassInfo),
    Recover(MetaPasswordId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {}
