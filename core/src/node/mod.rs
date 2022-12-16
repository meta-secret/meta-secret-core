use reqwest::Error;

use crate::sdk::api::{
    membership::MembershipResponse, GenericMessage, JoinRequest, MetaPasswordsResponse, PasswordRecoveryClaimsResponse,
    PasswordRecoveryRequest, RegistrationResponse, SecretDistributionDocData, UserSharesResponse, VaultInfoResponse,
};
use crate::sdk::vault::UserSignature;

const API_URL: &str = "http://api.meta-secret.org";

/// Register new vault
pub async fn register(user_sig: &UserSignature) -> Result<RegistrationResponse, Error> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/register", API_URL))
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: RegistrationResponse = response.json().await?;
    Ok(json)
}

pub async fn get_vault(user_sig: &UserSignature) -> Result<VaultInfoResponse, Error> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/getVault", API_URL))
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: VaultInfoResponse = response.json().await?;
    Ok(json)
}

pub async fn decline(request: &JoinRequest) -> Result<MembershipResponse, Error> {
    todo!("not implemented")
}

pub async fn accept(request: &JoinRequest) -> Result<MembershipResponse, Error> {
    todo!("not implemented")
}

pub async fn claim_for_password_recovery(request: &PasswordRecoveryRequest) -> Result<PasswordRecoveryRequest, Error> {
    todo!("not implemented")
}

pub async fn find_password_recovery_claims(user_sig: &UserSignature) -> Result<PasswordRecoveryClaimsResponse, Error> {
    todo!("not implemented")
}

pub async fn distribute(secret_doc: &SecretDistributionDocData) -> Result<GenericMessage<String>, Error> {
    todo!("not implemented")
}

pub async fn find_shares(user_sig: &UserSignature) -> Result<UserSharesResponse, Error> {
    todo!("not implemented")
}

pub async fn get_meta_passwords(user_sig: &UserSignature) -> Result<MetaPasswordsResponse, Error> {
    todo!("not implemented")
}

///cloud
pub async fn join_meta_cloud(user_sig: &UserSignature) -> Result<RegistrationResponse, Error> {
    todo!("not implemented")
}
