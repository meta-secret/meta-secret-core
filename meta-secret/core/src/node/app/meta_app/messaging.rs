use crate::node::common::model::ApplicationState;
use crate::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use crate::node::common::model::vault::vault::VaultName;

#[derive(Clone, Debug, PartialEq)]
pub enum GenericAppStateRequest {
    GetState,
    GenerateUserCreds(VaultName),
    SignUp(VaultName),
    ClusterDistribution(PlainPassInfo),
    Recover(MetaPasswordId),
    DeleteSecret(MetaPasswordId),
    DecideDeleteSecret { request_id: String, accepted: bool },
    RemoveMember(String),
    ApproveRemoveMember(String),
    DeclineRemoveMember(String),
}

impl GenericAppStateRequest {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::GetState => "GetState",
            Self::GenerateUserCreds(_) => "GenerateUserCreds",
            Self::SignUp(_) => "SignUp",
            Self::ClusterDistribution(_) => "ClusterDistribution",
            Self::Recover(_) => "Recover",
            Self::DeleteSecret(_) => "DeleteSecret",
            Self::DecideDeleteSecret { .. } => "DecideDeleteSecret",
            Self::RemoveMember(_) => "RemoveMember",
            Self::ApproveRemoveMember(_) => "ApproveRemoveMember",
            Self::DeclineRemoveMember(_) => "DeclineRemoveMember",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GenericAppStateResponse {
    AppState(ApplicationState),
    Error(String),
}
