use reqwest::{Client, Error, Response};

use crate::models::{JoinRequest, MetaPasswordRequest, PasswordRecoveryRequest, SecretDistributionDocData, UserSignature};
use crate::sdk::api::{GenericMessage, MembershipResponse, MetaPasswordsResponse, PasswordRecoveryClaimsResponse, RegistrationResponse, UserSharesResponse, VaultInfoResponse};

const API_URL: &str = "https://api.meta-secret.org";

/// Register new vault
pub async fn register(user_sig: &UserSignature) -> Result<RegistrationResponse, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/register", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: RegistrationResponse = response.json().await?;
    Ok(json)
}

pub async fn get_vault(user_sig: &UserSignature) -> Result<VaultInfoResponse, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/getVault", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: VaultInfoResponse = response.json().await?;
    Ok(json)
}

pub async fn decline(join_request: &JoinRequest) -> Result<MembershipResponse, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/decline", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(join_request)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: MembershipResponse = response.json().await?;
    Ok(json)
}

pub async fn accept(request: &JoinRequest) -> Result<MembershipResponse, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/accept", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(request)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: MembershipResponse = response.json().await?;
    Ok(json)
}

pub async fn claim_for_password_recovery(request: &PasswordRecoveryRequest) -> Result<PasswordRecoveryRequest, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/claimForPasswordRecovery", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(request)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: PasswordRecoveryRequest = response.json().await?;
    Ok(json)
}

pub async fn find_password_recovery_claims(user_sig: &UserSignature) -> Result<PasswordRecoveryClaimsResponse, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/findPasswordRecoveryClaims", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: PasswordRecoveryClaimsResponse = response.json().await?;
    Ok(json)
}

pub async fn distribute(secret_doc: &SecretDistributionDocData) -> Result<GenericMessage<String>, Error> {
    let client = get_reqwest_client();
    let response = client
        .post(format!("{}/distribute", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(secret_doc)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: GenericMessage<String> = response.json().await?;
    Ok(json)
}

pub async fn find_shares(user_sig: &UserSignature) -> Result<UserSharesResponse, Error> {
    let client = get_reqwest_client();
    let response: Response = client
        .post(format!("{}/findShares", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: UserSharesResponse = response.json().await?;
    Ok(json)
}

pub async fn get_meta_passwords(user_sig: &UserSignature) -> Result<MetaPasswordsResponse, Error> {
    let client = get_reqwest_client();
    let response: Response = client
        .post(format!("{}/getMetaPasswords", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: MetaPasswordsResponse = response.json().await?;
    Ok(json)
}

pub async fn delete_meta_password(meta_pass_request: &MetaPasswordRequest) -> Result<MetaPasswordsResponse, Error> {
    let client = get_reqwest_client();
    let response: Response = client
        .post(format!("{}/deleteMetaPassword", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(meta_pass_request)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: MetaPasswordsResponse = response.json().await?;
    Ok(json)
}

///cloud
pub async fn join_meta_cloud(user_sig: &UserSignature) -> Result<RegistrationResponse, Error> {
    let client = get_reqwest_client();
    let response: Response = client
        .post(format!("{}/joinMetaCloud", API_URL))
        .header("Access-Control-Allow-Origin", API_URL)
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: RegistrationResponse = response.json().await?;
    Ok(json)
}


fn get_reqwest_client() -> Client {
    Client::new()
}
