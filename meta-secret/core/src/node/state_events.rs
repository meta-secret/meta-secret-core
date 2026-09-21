use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::DsaKeyPair;
use crate::crypto::keys::DsaPk;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::security::canonical_json;
use anyhow::{Result, anyhow, bail};
use base64::Engine;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now_millis() -> f64;
}

/// A short-lived, device-signed credential for the state invalidation stream.
///
/// The credential identifies only the subscribing device and Vault. It does
/// not contain application state, Secrets, Key Shares, or private key material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEventsSubscription {
    pub signer: DeviceId,
    pub vault_name: VaultName,
    pub issued_at: u64,
    pub expires_at: u64,
    pub nonce: String,
    pub signature: Base64Text,
}

#[derive(Serialize)]
struct UnsignedStateEventsSubscription<'a> {
    signer: &'a DeviceId,
    vault_name: &'a VaultName,
    issued_at: u64,
    expires_at: u64,
    nonce: &'a str,
}

impl StateEventsSubscription {
    pub const TOKEN_TTL_SECS: u64 = 300;
    pub const CLOCK_SKEW_SECS: u64 = 30;

    pub fn sign(
        vault_name: VaultName,
        signer: DeviceId,
        key_pair: &DsaKeyPair,
    ) -> Result<Self> {
        Self::sign_at(vault_name, signer, key_pair, unix_time_secs()?)
    }

    pub fn sign_at(
        vault_name: VaultName,
        signer: DeviceId,
        key_pair: &DsaKeyPair,
        issued_at: u64,
    ) -> Result<Self> {
        let mut nonce_bytes = [0u8; 24];
        OsRng
            .try_fill_bytes(&mut nonce_bytes)
            .map_err(|error| anyhow!("failed to generate subscription nonce: {error}"))?;
        let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce_bytes);
        let mut subscription = Self {
            signer,
            vault_name,
            issued_at,
            expires_at: issued_at + Self::TOKEN_TTL_SECS,
            nonce,
            signature: Base64Text::from(""),
        };
        subscription.signature = key_pair.sign(String::from_utf8(
            subscription.unsigned_canonical_bytes()?,
        )?);
        Ok(subscription)
    }

    pub fn verify(&self, public_key: &DsaPk, now: u64) -> Result<()> {
        if self.expires_at <= now {
            bail!("state events subscription has expired");
        }
        if self.issued_at > now.saturating_add(Self::CLOCK_SKEW_SECS) {
            bail!("state events subscription is not valid yet");
        }
        if self.expires_at < self.issued_at
            || self.expires_at - self.issued_at > Self::TOKEN_TTL_SECS + Self::CLOCK_SKEW_SECS
        {
            bail!("state events subscription lifetime is invalid");
        }
        let canonical = String::from_utf8(self.unsigned_canonical_bytes()?)?;
        public_key.verify(&canonical, &self.signature)
    }

    pub fn unsigned_canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_json(&UnsignedStateEventsSubscription {
            signer: &self.signer,
            vault_name: &self.vault_name,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            nonce: &self.nonce,
        })
    }

    pub fn bearer_token(&self) -> Result<String> {
        let json = serde_json::to_vec(self)?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
    }

    pub fn from_bearer_token(token: &str) -> Result<Self> {
        let encoded = token
            .strip_prefix("Bearer ")
            .unwrap_or(token)
            .trim();
        if encoded.is_empty() {
            bail!("state events bearer token is empty");
        }
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|error| anyhow!("invalid state events bearer token: {error}"))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| anyhow!("invalid state events subscription: {error}"))
    }
}

pub fn unix_time_secs() -> Result<u64> {
    #[cfg(target_arch = "wasm32")]
    {
        let millis = date_now_millis();
        if !millis.is_finite() || millis < 0.0 {
            bail!("browser clock returned an invalid Unix timestamp");
        }
        return Ok((millis / 1_000.0).floor() as u64);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| anyhow!("system clock is before Unix epoch: {error}"))?
            .as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::key_pair::{KeyPair, DsaKeyPair};
    use crate::crypto::utils::U64IdUrlEnc;
    use crate::node::common::model::vault::vault::VaultName;

    fn signer() -> (DeviceId, DsaKeyPair) {
        let key_pair = DsaKeyPair::generate();
        (
            DeviceId(U64IdUrlEnc::from("state-events-device".to_string())),
            key_pair,
        )
    }

    #[test]
    fn signs_verifies_and_roundtrips_bearer_token() {
        let (device_id, key_pair) = signer();
        let subscription = StateEventsSubscription::sign_at(
            VaultName::from("vault-a"),
            device_id,
            &key_pair,
            1_000,
        )
        .unwrap();

        subscription.verify(&key_pair.pk(), 1_001).unwrap();
        let token = subscription.bearer_token().unwrap();
        let decoded = StateEventsSubscription::from_bearer_token(&token).unwrap();
        assert_eq!(decoded, subscription);
        decoded.verify(&key_pair.pk(), 1_001).unwrap();
    }

    #[test]
    fn rejects_tampered_vault_and_signature() {
        let (device_id, key_pair) = signer();
        let mut subscription = StateEventsSubscription::sign_at(
            VaultName::from("vault-a"),
            device_id,
            &key_pair,
            1_000,
        )
        .unwrap();
        subscription.vault_name = VaultName::from("vault-b");
        assert!(subscription.verify(&key_pair.pk(), 1_001).is_err());
    }

    #[test]
    fn rejects_expired_and_not_yet_valid_credentials() {
        let (device_id, key_pair) = signer();
        let subscription = StateEventsSubscription::sign_at(
            VaultName::from("vault-a"),
            device_id,
            &key_pair,
            1_000,
        )
        .unwrap();
        assert!(subscription.verify(&key_pair.pk(), 1_301).is_err());
        assert!(subscription.verify(&key_pair.pk(), 900).is_err());
    }
}
