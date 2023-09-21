use crate::models::MetaPasswordId;

pub enum GenericAppStateRequest {
    SignUp(SignUpRequest),
    Recover(RecoveryRequest),
    ClusterDistribution(ClusterDistributionRequest),
}

pub struct SignUpRequest {
    pub vault_name: String,
    pub device_name: String,
}

pub struct RecoveryRequest {
    pub meta_pass_id: MetaPasswordId,
}

pub struct ClusterDistributionRequest {
    pub pass_id: String,
    pub pass: String,
}

pub enum GenericAppStateResponse {}
