use crate::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use crate::node::common::model::vault::vault::VaultName;
use crate::node::common::model::ApplicationState;

#[derive(Clone, Debug, PartialEq)]
pub enum GenericAppStateRequest {
    GetState,
    GenerateUserCreds(VaultName),
    SignUp(VaultName),
    ClusterDistribution(PlainPassInfo),
    Recover(MetaPasswordId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {
    AppState(ApplicationState),
}
