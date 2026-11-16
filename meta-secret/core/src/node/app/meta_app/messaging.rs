use crate::node::api::CanonicalDecisionInput;
use crate::node::common::model::ApplicationState;
use crate::node::common::model::liveboard::LiveboardOperationReceipt;
use crate::node::common::model::liveboard_producer::{LiveboardProducerReceipt, ProducerReference};
use crate::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use crate::node::common::model::vault::vault::VaultName;

#[derive(Clone, Debug, PartialEq)]
pub enum GenericAppStateRequest {
    GetState,
    DeleteSecretChecked(MetaPasswordId),
    RemoveMemberChecked(String),
    ReconcileProducerReceipt(ProducerReference),
    ContinueLocalRemoveChecked(ProducerReference),
    ClusterDistributionChecked(PlainPassInfo),
    RecoverChecked(MetaPasswordId),
    DecideJoinRequest(CanonicalDecisionInput),
    DecideRemoveRequest(CanonicalDecisionInput),
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
    pub fn is_producer(&self) -> bool {
        matches!(
            self,
            Self::DeleteSecretChecked(_)
                | Self::RemoveMemberChecked(_)
                | Self::ReconcileProducerReceipt(_)
                | Self::ContinueLocalRemoveChecked(_)
        )
    }
    pub fn is_checked(&self) -> bool {
        self.is_producer()
            || matches!(
                self,
                Self::ClusterDistributionChecked(_)
                    | Self::RecoverChecked(_)
                    | Self::DecideJoinRequest(_)
                    | Self::DecideRemoveRequest(_)
            )
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::DeleteSecretChecked(_) => "DeleteSecretChecked",
            Self::RemoveMemberChecked(_) => "RemoveMemberChecked",
            Self::ReconcileProducerReceipt(_) => "ReconcileProducerReceipt",
            Self::ContinueLocalRemoveChecked(_) => "ContinueLocalRemoveChecked",
            Self::ClusterDistributionChecked(_) => "ClusterDistributionChecked",
            Self::RecoverChecked(_) => "RecoverChecked",
            Self::DecideJoinRequest(_) => "DecideJoinRequest",
            Self::DecideRemoveRequest(_) => "DecideRemoveRequest",
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
    OperationReceipt(LiveboardOperationReceipt),
    ProducerReceipt(LiveboardProducerReceipt),
    Error(String),
}
