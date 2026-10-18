use super::EmailVerifier;
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

const JWKS: &str = "https://appleid.apple.com/auth/keys";
const ISSUER: &str = "https://appleid.apple.com";
#[derive(Clone, Deserialize)]
struct Key {
    kid: String,
    kty: String,
    alg: String,
    r#use: String,
    n: String,
    e: String,
}
#[derive(Default)]
struct Cache {
    keys: Vec<Key>,
    fetched: u64,
    attempted: u64,
}
pub(super) struct AppleVerifier {
    audience: String,
    client: reqwest::Client,
    cache: Mutex<Cache>,
}
impl AppleVerifier {
    pub(super) fn new(audience: String) -> anyhow::Result<Self> {
        Ok(Self {
            audience,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(5))
                .build()?,
            cache: Mutex::new(Cache::default()),
        })
    }
    async fn key(&self, kid: &str) -> Result<DecodingKey, ()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_secs();
        self.key_with_fetch(kid, now, || self.fetch_keys()).await
    }
    async fn key_with_fetch<F, Fut>(&self, kid: &str, now: u64, fetch: F) -> Result<DecodingKey, ()>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<Vec<Key>, ()>> + Send,
    {
        let mut cache = tokio::time::timeout(Duration::from_secs(6), self.cache.lock())
            .await
            .map_err(|_| ())?;
        if now.saturating_sub(cache.fetched) < 3600
            && let Some(key) = cache.keys.iter().find(|key| key.kid == kid)
        {
            return decoding_key(key);
        }
        if now.saturating_sub(cache.attempted) < 30 {
            return Err(());
        }
        cache.attempted = now;
        let keys = fetch().await?;
        if keys.is_empty()
            || keys.len() > 16
            || keys
                .iter()
                .any(|key| key.kid.is_empty() || key.kid.len() > 128)
            || keys
                .iter()
                .map(|key| &key.kid)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != keys.len()
        {
            return Err(());
        }
        cache.keys = keys;
        cache.fetched = now;
        decoding_key(cache.keys.iter().find(|key| key.kid == kid).ok_or(())?)
    }
    async fn fetch_keys(&self) -> Result<Vec<Key>, ()> {
        let mut response = self.client.get(JWKS).send().await.map_err(|_| ())?;
        keys_from_body(
            response.status().as_u16(),
            response.content_length(),
            &mut response,
        )
        .await
    }
}
#[async_trait]
trait Chunks: Send {
    async fn next(&mut self) -> Result<Option<axum::body::Bytes>, ()>;
}
#[async_trait]
impl Chunks for reqwest::Response {
    async fn next(&mut self) -> Result<Option<axum::body::Bytes>, ()> {
        self.chunk().await.map_err(|_| ())
    }
}
async fn keys_from_body(
    status: u16,
    length: Option<u64>,
    body: &mut impl Chunks,
) -> Result<Vec<Key>, ()> {
    if !(200..300).contains(&status) || length.is_some_and(|size| size > 256 * 1024) {
        return Err(());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await? {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err(());
        }
        bytes.extend_from_slice(&chunk);
    }
    #[derive(Deserialize)]
    struct Keys {
        keys: Vec<Key>,
    }
    Ok(serde_json::from_slice::<Keys>(&bytes).map_err(|_| ())?.keys)
}
fn decoding_key(key: &Key) -> Result<DecodingKey, ()> {
    if key.kty != "RSA"
        || key.alg != "RS256"
        || key.r#use != "sig"
        || key.n.len() > 4096
        || key.e.len() > 16
    {
        return Err(());
    }
    DecodingKey::from_rsa_components(&key.n, &key.e).map_err(|_| ())
}
#[derive(Clone, Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    sub: String,
    nonce: String,
    exp: u64,
    iat: u64,
    email: String,
    email_verified: serde_json::Value,
    c_hash: String,
}
fn verified_email(
    token: &str,
    code: &str,
    nonce: &str,
    created: u64,
    now: u64,
    audience: &str,
    key: &DecodingKey,
) -> Result<String, ()> {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[audience]);
    validation.leeway = 0;
    validation.validate_nbf = true;
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat"]);
    let claims = decode::<Claims>(token, key, &validation)
        .map_err(|_| ())?
        .claims;
    let c_hash = URL_SAFE_NO_PAD.encode(&Sha256::digest(code.as_bytes())[..16]);
    if claims.iss != ISSUER
        || claims.aud != audience
        || claims.sub.is_empty()
        || claims.nonce != nonce
        || claims.exp <= now
        || claims.iat > now.saturating_add(30)
        || claims.iat.saturating_add(30) < created
        || claims.c_hash != c_hash
        || !matches!(claims.email_verified, serde_json::Value::Bool(true))
            && claims.email_verified != "true"
        || claims.email.is_empty()
        || claims.email.len() > 320
        || !claims.email.contains('@')
        || claims.email.chars().any(char::is_control)
    {
        return Err(());
    }
    Ok(claims.email)
}
#[async_trait]
impl EmailVerifier for AppleVerifier {
    async fn verify(
        &self,
        token: &str,
        code: &str,
        nonce: &str,
        created: u64,
    ) -> Result<String, ()> {
        let header = decode_header(token).map_err(|_| ())?;
        if header.alg != Algorithm::RS256
            || header.jku.is_some()
            || header.x5u.is_some()
            || header.jwk.is_some()
        {
            return Err(());
        }
        let kid = header
            .kid
            .filter(|kid| !kid.is_empty() && kid.len() <= 128)
            .ok_or(())?;
        let key = self.key(&kid).await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_secs();
        verified_email(token, code, nonce, created, now, &self.audience, &key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use rsa::{
        RsaPrivateKey,
        pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey},
        traits::PublicKeyParts,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;
    fn signed(changes: Value) -> (String, DecodingKey, u64) {
        // Ephemeral synthetic test key, unrelated to any Apple/developer account.
        static KEY: std::sync::OnceLock<(EncodingKey, Vec<u8>)> = std::sync::OnceLock::new();
        let (encoding, public) = KEY.get_or_init(|| {
            let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
            (
                EncodingKey::from_rsa_der(private.to_pkcs1_der().unwrap().as_bytes()),
                private
                    .to_public_key()
                    .to_pkcs1_der()
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            )
        });
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut claims = json!({"iss":ISSUER,"aud":"service","sub":"synthetic","nonce":"nonce","exp":now+300,"iat":now,"email":"Exact@privaterelay.appleid.com","email_verified":"true","c_hash":URL_SAFE_NO_PAD.encode(&Sha256::digest(b"code")[..16])});
        for (key, value) in changes.as_object().unwrap() {
            claims[key] = value.clone();
        }
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("synthetic-key".into());
        (
            encode(&header, &claims, encoding).unwrap(),
            DecodingKey::from_rsa_der(public),
            now,
        )
    }
    #[test]
    fn signed_claims_only_and_exact_relay() {
        let (token, key, now) = signed(json!({}));
        assert_eq!(
            verified_email(&token, "code", "nonce", now, now, "service", &key),
            Ok("Exact@privaterelay.appleid.com".into())
        );
        assert!(verified_email(&token, "other-code", "nonce", now, now, "service", &key).is_err());
        let mut tampered = token.into_bytes();
        let index = tampered.len() - 4;
        tampered[index] = if tampered[index] == b'A' { b'B' } else { b'A' };
        assert!(
            verified_email(
                &String::from_utf8(tampered).unwrap(),
                "code",
                "nonce",
                now,
                now,
                "service",
                &key
            )
            .is_err()
        );
    }
    #[test]
    fn invalid_signed_claims_fail_closed() {
        for change in [
            json!({"iss":"https://evil.test"}),
            json!({"aud":"other"}),
            json!({"nonce":"wrong"}),
            json!({"sub":""}),
            json!({"exp":1}),
            json!({"iat":1}),
            json!({"iat":u64::MAX}),
            json!({"nbf":u64::MAX/2}),
            json!({"email_verified":false}),
            json!({"email":""}),
            json!({"email":"x\ny@example.com"}),
            json!({"c_hash":"wrong"}),
        ] {
            let (token, key, now) = signed(change.clone());
            assert!(
                verified_email(&token, "code", "nonce", now, now, "service", &key).is_err(),
                "{change}"
            );
        }
    }
    #[tokio::test]
    async fn unsupported_algorithms_and_unknown_kid_fail_without_network() {
        let verifier = Arc::new(AppleVerifier::new("service".into()).unwrap());
        let token = encode(
            &Header::new(Algorithm::HS256),
            &json!({}),
            &EncodingKey::from_secret(b"synthetic"),
        )
        .unwrap();
        assert!(verifier.verify(&token, "code", "nonce", 0).await.is_err());
        for header in [
            json!({"alg":"none","kid":"synthetic-key"}),
            json!({"alg":"RS256","kid":"synthetic-key","jku":"https://evil.test/keys"}),
        ] {
            let token = format!("{}.e30.eA", URL_SAFE_NO_PAD.encode(header.to_string()));
            assert!(verifier.verify(&token, "code", "nonce", 0).await.is_err());
        }
        verifier.cache.lock().await.attempted = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let unknown = format!(
            "{}.e30.eA",
            URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","kid":"unknown-key"}"#)
        );
        assert!(verifier.verify(&unknown, "code", "nonce", 0).await.is_err());
        assert!(verifier.cache.lock().await.keys.is_empty());
    }
    fn public_key() -> Key {
        // Create an independent ephemeral public fixture for cache-only tests.
        static KEY: std::sync::OnceLock<Key> = std::sync::OnceLock::new();
        KEY.get_or_init(|| {
            let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
            let public = private.to_public_key();
            Key {
                kid: "synthetic-key".into(),
                kty: "RSA".into(),
                alg: "RS256".into(),
                r#use: "sig".into(),
                n: URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
                e: URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
            }
        })
        .clone()
    }
    #[tokio::test]
    async fn bounded_cache_rotation_cooldown_and_failed_refresh() {
        let verifier = AppleVerifier::new("service".into()).unwrap();
        let key = public_key();
        assert!(
            verifier
                .key_with_fetch("synthetic-key", 1000, || async { Ok(vec![key.clone()]) })
                .await
                .is_ok()
        );
        assert!(
            verifier
                .key_with_fetch("synthetic-key", 1001, || async {
                    panic!("unexpected network")
                })
                .await
                .is_ok()
        );
        assert!(
            verifier
                .key_with_fetch("new-key", 1001, || async { panic!("cooldown network") })
                .await
                .is_err()
        );
        let mut rotated = key;
        rotated.kid = "new-key".into();
        assert!(
            verifier
                .key_with_fetch("new-key", 1031, || async { Ok(vec![rotated]) })
                .await
                .is_ok()
        );
        assert!(
            verifier
                .key_with_fetch("missing", 1062, || async { Err(()) })
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn unknown_length_oversize_body_stops_reading_and_redirect_reads_nothing() {
        struct Body {
            calls: usize,
        }
        #[async_trait]
        impl Chunks for Body {
            async fn next(&mut self) -> Result<Option<axum::body::Bytes>, ()> {
                self.calls += 1;
                Ok(Some(axum::body::Bytes::from(vec![0; 100_000])))
            }
        }
        let mut body = Body { calls: 0 };
        assert!(keys_from_body(302, None, &mut body).await.is_err());
        assert_eq!(body.calls, 0);
        assert!(keys_from_body(200, None, &mut body).await.is_err());
        assert_eq!(body.calls, 3);
    }
    #[tokio::test]
    async fn actual_router_uses_real_signature_and_ignores_unsigned_email() {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt;
        // Obtain a synthetic signed token and its matching public key without external IO.
        let (_, _, now) = signed(json!({}));
        let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let public = private.to_public_key();
        let key = Key {
            kid: "router-key".into(),
            kty: "RSA".into(),
            alg: "RS256".into(),
            r#use: "sig".into(),
            n: URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            e: URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        };
        let verifier = AppleVerifier::new("service".into()).unwrap();
        {
            let mut cache = verifier.cache.lock().await;
            cache.keys = vec![key];
            cache.fetched = now;
            cache.attempted = now;
        }
        let config = super::super::Config::parse(
            "service",
            "https://broker.example.test/apple/callback",
            "http://localhost:5174",
        )
        .unwrap();
        let broker = super::super::Broker::new(config, Arc::new(verifier), Arc::new(move || now));
        let proof = "A".repeat(43);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(proof.as_bytes()));
        let response = broker
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/apple/start")
                    .header("content-type", "application/json")
                    .header("origin", "http://localhost:5174")
                    .body(Body::from(
                        json!({"platform":"web","challenge":challenge}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let start: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
        let url = reqwest::Url::parse(start["authorizationUrl"].as_str().unwrap()).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        let claims = json!({"iss":ISSUER,"aud":"service","sub":"synthetic","nonce":params["nonce"],"exp":now+300,"iat":now,"email":"Signed@privaterelay.appleid.com","email_verified":true,"c_hash":URL_SAFE_NO_PAD.encode(&Sha256::digest(b"code")[..16])});
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("router-key".into());
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_der(private.to_pkcs1_der().unwrap().as_bytes()),
        )
        .unwrap();
        let form = reqwest::Url::parse_with_params(
            "https://example.test",
            [
                ("state", params["state"].as_str()),
                ("code", "code"),
                ("id_token", token.as_str()),
                ("user", "{\"email\":\"forged@example.test\"}"),
            ],
        )
        .unwrap()
        .query()
        .unwrap()
        .to_owned();
        let response = broker
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/apple/callback")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let response = broker
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/apple/take")
                    .header("content-type", "application/json")
                    .header("origin", "http://localhost:5174")
                    .body(Body::from(
                        json!({"requestId":start["requestId"],"verifier":proof}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let result: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap();
        assert_eq!(
            result,
            json!({"status":"success","email":"Signed@privaterelay.appleid.com"})
        );
    }
}
