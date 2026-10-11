//! Request-scoped credentials for observing the terminal result of a JOIN.

use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::key_pair::DsaKeyPair;
use crate::crypto::keys::DsaPk;
use crate::node::common::model::device::common::DeviceId;
use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::VaultName;
use crate::node::db::events::vault::vault_log_event::JoinClusterEvent;
use crate::node::security::canonical_json;
use crate::node::state_events::unix_time_secs;
use anyhow::{Result, anyhow, bail};
use base64::Engine;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// Short-lived credential for the terminal result of one JOIN request.
/// `request_id` is the Core-generated random identifier embedded in the JOIN
/// request/update relationship.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinResultSubscription {
    pub signer: DeviceId,
    pub vault_name: VaultName,
    pub request_id: String,
    pub signer_public_key: DsaPk,
    pub issued_at: u64,
    pub expires_at: u64,
    pub nonce: String,
    pub signature: Base64Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinResultStatus {
    Pending,
    Accepted,
    Declined,
}

#[derive(Serialize)]
struct UnsignedJoinResultSubscription<'a> {
    signer: &'a DeviceId,
    vault_name: &'a VaultName,
    request_id: &'a str,
    signer_public_key: &'a DsaPk,
    issued_at: u64,
    expires_at: u64,
    nonce: &'a str,
}

impl JoinResultSubscription {
    pub const TOKEN_TTL_SECS: u64 = 300;
    pub const CLOCK_SKEW_SECS: u64 = 30;

    pub fn sign(request: &JoinClusterEvent, key_pair: &DsaKeyPair) -> Result<Self> {
        Self::sign_at(request, key_pair, unix_time_secs()?)
    }

    pub fn sign_at(
        request: &JoinClusterEvent,
        key_pair: &DsaKeyPair,
        issued_at: u64,
    ) -> Result<Self> {
        if request.request_id.is_empty() {
            bail!("legacy JOIN requests cannot receive a request-scoped subscription");
        }
        let mut nonce_bytes = [0u8; 24];
        OsRng
            .try_fill_bytes(&mut nonce_bytes)
            .map_err(|error| anyhow!("failed to generate join subscription nonce: {error}"))?;
        let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce_bytes);
        let mut subscription = Self {
            signer: request.candidate.device.device_id.clone(),
            vault_name: request.candidate.vault_name(),
            request_id: request.request_id.clone(),
            signer_public_key: request.candidate.device.keys.dsa_pk.clone(),
            issued_at,
            expires_at: issued_at + Self::TOKEN_TTL_SECS,
            nonce,
            signature: Base64Text::from(""),
        };
        subscription.signature =
            key_pair.sign(String::from_utf8(subscription.unsigned_canonical_bytes()?)?);
        Ok(subscription)
    }

    pub fn verify(
        &self,
        candidate: &UserData,
        request_id: &str,
        public_key: &DsaPk,
        now: u64,
    ) -> Result<()> {
        self.verify_signature(now)?;
        if candidate.device.device_id != self.signer
            || DeviceId::from(&candidate.device.keys) != self.signer
            || candidate.vault_name() != self.vault_name
        {
            bail!("join result subscription identity does not match candidate");
        }
        if request_id.is_empty() || self.request_id != request_id {
            bail!("join result subscription does not match request");
        }
        if &self.signer_public_key != public_key {
            bail!("join result subscription key does not match canonical candidate");
        }
        Ok(())
    }

    /// Authenticate the bearer token without consulting persisted Vault state.
    /// The caller must still compare the signed public key to the canonical
    /// candidate key after loading membership.
    pub fn verify_signature(&self, now: u64) -> Result<()> {
        if self.request_id.is_empty() {
            bail!("legacy JOIN requests cannot receive a request-scoped subscription");
        }
        if self.expires_at <= now {
            bail!("join result subscription has expired");
        }
        if self.issued_at > now.saturating_add(Self::CLOCK_SKEW_SECS) {
            bail!("join result subscription is not valid yet");
        }
        if self.expires_at < self.issued_at
            || self.expires_at - self.issued_at > Self::TOKEN_TTL_SECS + Self::CLOCK_SKEW_SECS
        {
            bail!("join result subscription lifetime is invalid");
        }
        let canonical = String::from_utf8(self.unsigned_canonical_bytes()?)?;
        self.signer_public_key.verify(&canonical, &self.signature)
    }

    pub fn unsigned_canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_json(&UnsignedJoinResultSubscription {
            signer: &self.signer,
            vault_name: &self.vault_name,
            request_id: &self.request_id,
            signer_public_key: &self.signer_public_key,
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
        let encoded = token.strip_prefix("Bearer ").unwrap_or(token).trim();
        if encoded.is_empty() {
            bail!("join result bearer token is empty");
        }
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|error| anyhow!("invalid join result bearer token: {error}"))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| anyhow!("invalid join result subscription: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::key_pair::KeyPair;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;

    #[test]
    fn signs_roundtrips_and_verifies_a_request_bound_token() {
        let registry = FixtureRegistry::empty();
        let candidate = registry.state.user_creds.vd.user();
        let request = JoinClusterEvent {
            candidate: candidate.clone(),
            request_id: "request-1".into(),
        };
        let key_pair = registry
            .state
            .user_creds
            .vd
            .device_creds
            .key_manager()
            .unwrap()
            .dsa;
        let subscription = JoinResultSubscription::sign_at(&request, &key_pair, 1_000).unwrap();

        subscription.verify_signature(1_001).unwrap();
        subscription
            .verify(&candidate, "request-1", &key_pair.pk(), 1_001)
            .unwrap();
        let token = subscription.bearer_token().unwrap();
        let decoded = JoinResultSubscription::from_bearer_token(&token).unwrap();
        assert_eq!(decoded, subscription);
        decoded
            .verify(&candidate, "request-1", &key_pair.pk(), 1_001)
            .unwrap();
    }

    #[test]
    fn rejects_another_candidate_tampering_and_expired_tokens() {
        let registry = FixtureRegistry::empty();
        let candidate = registry.state.user_creds.vd.user();
        let request = JoinClusterEvent {
            candidate: candidate.clone(),
            request_id: "request-2".into(),
        };
        let key_pair = registry
            .state
            .user_creds
            .vd
            .device_creds
            .key_manager()
            .unwrap()
            .dsa;
        let subscription = JoinResultSubscription::sign_at(&request, &key_pair, 1_000).unwrap();

        assert!(
            subscription
                .verify(&candidate, "request-2", &key_pair.pk(), 1_301)
                .is_err()
        );
        assert!(
            subscription
                .verify(&candidate, "request-2", &key_pair.pk(), 900)
                .is_err()
        );
        assert!(
            subscription
                .verify(
                    &registry.state.user_creds.client.user(),
                    "request-2",
                    &key_pair.pk(),
                    1_001
                )
                .is_err()
        );

        let mut tampered = subscription;
        tampered.vault_name = VaultName::from("other-vault");
        assert!(
            tampered
                .verify(&candidate, "request-2", &key_pair.pk(), 1_001)
                .is_err()
        );
        assert!(tampered.verify_signature(1_001).is_err());
    }

    #[test]
    fn signed_public_key_must_match_canonical_candidate_key() {
        let registry = FixtureRegistry::empty();
        let candidate = registry.state.user_creds.vd.user();
        let request = JoinClusterEvent {
            candidate: candidate.clone(),
            request_id: "request-key-binding".into(),
        };
        let key_pair = registry
            .state
            .user_creds
            .vd
            .device_creds
            .key_manager()
            .unwrap()
            .dsa;
        let subscription = JoinResultSubscription::sign_at(&request, &key_pair, 1_000).unwrap();
        let other_key = crate::crypto::key_pair::DsaKeyPair::generate();

        // The token signature is independently valid, but the server must
        // reject it after loading canonical candidate state if keys differ.
        assert!(subscription.verify_signature(1_001).is_ok());
        assert!(
            subscription
                .verify(&candidate, "request-key-binding", &other_key.pk(), 1_001)
                .is_err()
        );
    }

    #[test]
    fn stale_request_credential_cannot_follow_a_later_join_attempt() {
        let registry = FixtureRegistry::empty();
        let candidate = registry.state.user_creds.vd.user();
        let key_pair = registry
            .state
            .user_creds
            .vd
            .device_creds
            .key_manager()
            .unwrap()
            .dsa;
        let old_request = JoinClusterEvent {
            candidate: candidate.clone(),
            request_id: "old-request".into(),
        };
        let new_request = JoinClusterEvent {
            candidate: candidate.clone(),
            request_id: "new-request".into(),
        };
        let old_subscription =
            JoinResultSubscription::sign_at(&old_request, &key_pair, 1_000).unwrap();

        assert!(
            old_subscription
                .verify(&candidate, &new_request.request_id, &key_pair.pk(), 1_001)
                .is_err()
        );
        assert!(
            JoinResultSubscription::sign_at(
                &JoinClusterEvent {
                    candidate,
                    request_id: String::new()
                },
                &key_pair,
                1_000,
            )
            .is_err()
        );
    }
}
