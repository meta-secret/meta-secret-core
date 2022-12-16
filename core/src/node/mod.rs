use reqwest::Error;

use crate::sdk::api::RegistrationResponse;
use crate::sdk::vault::UserSignature;

/// Register new vault
pub async fn register(user_sig: &UserSignature) -> Result<RegistrationResponse, Error> {
    let client = reqwest::Client::new();
    let response = client
        .post("http://api.meta-secret.org/register")
        .json(user_sig)
        .send()
        .await?;

    // Read the response body as a JSON object
    let json: RegistrationResponse = response.json().await?;
    Ok(json)
}
