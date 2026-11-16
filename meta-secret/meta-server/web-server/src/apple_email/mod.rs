//! Short-lived provider email transport. It never authenticates a Vault member.
mod token;

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Form, State,
        rejection::{FormRejection, JsonRejection},
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::post,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tower_http::cors::{AllowOrigin, CorsLayer};

const TTL: u64 = 300;
const CAPACITY: usize = 1000;
const ANDROID_RETURN: &str = "metasecret://apple-auth/callback";

#[derive(Clone)]
pub(crate) struct Config {
    service_id: String,
    callback: reqwest::Url,
    origins: HashSet<String>,
}
impl Config {
    pub(crate) fn parse(service_id: &str, callback: &str, origins: &str) -> anyhow::Result<Self> {
        let callback = reqwest::Url::parse(callback)?;
        anyhow::ensure!(
            !service_id.is_empty()
                && service_id.len() <= 255
                && service_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b)),
            "Invalid Apple email configuration"
        );
        anyhow::ensure!(
            secure_url(&callback) && callback.path() == "/apple/callback",
            "Invalid Apple email callback"
        );
        let origins = origins
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|value| {
                let url = reqwest::Url::parse(value)?;
                let local = url.scheme() == "http" && url.host_str() == Some("localhost");
                anyhow::ensure!(
                    (secure_url(&url) || local)
                        && url.path() == "/"
                        && url.query().is_none()
                        && url.fragment().is_none()
                        && url.username().is_empty()
                        && url.password().is_none(),
                    "Invalid Apple email origin"
                );
                Ok(url.origin().ascii_serialization())
            })
            .collect::<anyhow::Result<HashSet<_>>>()?;
        anyhow::ensure!(!origins.is_empty(), "Apple email origins required");
        Ok(Self {
            service_id: service_id.into(),
            callback,
            origins,
        })
    }
}
fn secure_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host_str().is_some_and(|host| {
            host != "localhost"
                && !host.ends_with(".localhost")
                && host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_err()
        })
}
pub(crate) fn configured_router() -> anyhow::Result<Router> {
    let values = [
        "APPLE_EMAIL_SERVICE_ID",
        "APPLE_EMAIL_CALLBACK_URL",
        "APPLE_EMAIL_WEB_ORIGINS",
    ]
    .map(|name| std::env::var(name).ok());
    if values.iter().all(Option::is_none) {
        return Ok(Router::new());
    }
    let [Some(service), Some(callback), Some(origins)] = values else {
        anyhow::bail!("Incomplete Apple email configuration");
    };
    let config = Config::parse(&service, &callback, &origins)
        .map_err(|_| anyhow::anyhow!("Invalid Apple email configuration"))?;
    let verifier = Arc::new(token::AppleVerifier::new(config.service_id.clone())?);
    Ok(Broker::new(
        config,
        verifier,
        Arc::new(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        }),
    )
    .router())
}
#[async_trait]
pub(crate) trait EmailVerifier: Send + Sync {
    async fn verify(
        &self,
        token: &str,
        code: &str,
        nonce: &str,
        created: u64,
    ) -> Result<String, ()>;
}
#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Platform {
    Web,
    Android,
}
#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Outcome {
    Pending,
    Success { email: String },
    Cancelled,
    Error { code: &'static str },
}
enum Phase {
    Pending,
    Verifying,
    Ready(Outcome),
}
struct Attempt {
    state: String,
    nonce: String,
    challenge: [u8; 32],
    created: u64,
    expires: u64,
    platform: Platform,
    origin: Option<String>,
    phase: Phase,
}
#[derive(Default)]
struct Store {
    attempts: HashMap<String, Attempt>,
    states: HashMap<String, String>,
    rate_window: u64,
    starts: usize,
}
impl Store {
    fn prune(&mut self, now: u64) {
        self.attempts.retain(|_, attempt| attempt.expires > now);
        self.states.retain(|_, id| self.attempts.contains_key(id));
    }
}
pub(crate) struct Broker {
    config: Config,
    verifier: Arc<dyn EmailVerifier>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    store: Mutex<Store>,
}
impl Broker {
    pub(crate) fn new(
        config: Config,
        verifier: Arc<dyn EmailVerifier>,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            verifier,
            clock,
            store: Mutex::new(Store::default()),
        })
    }
    pub(crate) fn router(self: Arc<Self>) -> Router {
        let origins = self
            .config
            .origins
            .iter()
            .map(|value| HeaderValue::from_str(value).expect("validated origin"))
            .collect::<Vec<_>>();
        let json_routes = Router::new()
            .route("/apple/start", post(start))
            .route("/apple/take", post(take))
            .route("/apple/cancel", post(cancel))
            .layer(DefaultBodyLimit::max(16 * 1024))
            .layer(
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(origins))
                    .allow_methods([Method::POST])
                    .allow_headers([header::CONTENT_TYPE]),
            );
        json_routes
            .merge(
                Router::new()
                    .route("/apple/callback", post(callback))
                    .layer(DefaultBodyLimit::max(32 * 1024)),
            )
            .with_state(self)
    }
    fn origin(&self, headers: &HeaderMap) -> Result<Option<String>, StatusCode> {
        let Some(value) = headers.get(header::ORIGIN) else {
            return Ok(None);
        };
        let origin = value.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
        if self.config.origins.contains(origin) {
            Ok(Some(origin.into()))
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    }
}
fn random() -> String {
    let mut bytes = [0; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn canonical(value: &str) -> Option<[u8; 32]> {
    if value.len() != 43 {
        return None;
    }
    let bytes: [u8; 32] = URL_SAFE_NO_PAD.decode(value).ok()?.try_into().ok()?;
    (URL_SAFE_NO_PAD.encode(bytes) == value).then_some(bytes)
}
fn error(status: StatusCode) -> Response {
    let mut response = (
        status,
        Json(serde_json::json!({"status":"error","code":"unavailable"})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn json(outcome: Outcome) -> Response {
    let mut response = Json(outcome).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn input_error(status: StatusCode) -> Response {
    error(if status == StatusCode::UNPROCESSABLE_ENTITY {
        StatusCode::BAD_REQUEST
    } else {
        status
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    platform: Platform,
    challenge: String,
}
async fn start(
    State(broker): State<Arc<Broker>>,
    headers: HeaderMap,
    payload: Result<Json<Start>, JsonRejection>,
) -> Response {
    let origin = match broker.origin(&headers) {
        Ok(value) => value,
        Err(status) => return error(status),
    };
    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(rejection) => return input_error(rejection.status()),
    };
    let Some(challenge) = canonical(&payload.challenge) else {
        return error(StatusCode::BAD_REQUEST);
    };
    if payload.platform == Platform::Web && origin.is_none() {
        return error(StatusCode::FORBIDDEN);
    }
    let now = (broker.clock)();
    let mut store = broker.store.lock().expect("Apple email store");
    store.prune(now);
    if now.saturating_sub(store.rate_window) >= 60 {
        store.rate_window = now;
        store.starts = 0;
    }
    if store.attempts.len() >= CAPACITY || store.starts >= CAPACITY {
        return error(StatusCode::TOO_MANY_REQUESTS);
    }
    let id = random();
    let state = random();
    let nonce = random();
    let mut url =
        reqwest::Url::parse("https://appleid.apple.com/auth/authorize").expect("fixed URL");
    url.query_pairs_mut().extend_pairs([
        ("client_id", broker.config.service_id.as_str()),
        ("redirect_uri", broker.config.callback.as_str()),
        ("response_type", "code id_token"),
        ("response_mode", "form_post"),
        ("scope", "email"),
        ("state", &state),
        ("nonce", &nonce),
    ]);
    store.states.insert(state.clone(), id.clone());
    store.starts += 1;
    store.attempts.insert(
        id.clone(),
        Attempt {
            state,
            nonce,
            challenge,
            created: now,
            expires: now + TTL,
            platform: payload.platform,
            origin,
            phase: Phase::Pending,
        },
    );
    let mut response = Json(
        serde_json::json!({"requestId": id,"authorizationUrl":url.as_str(),"expiresAt":now+TTL}),
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Proof {
    request_id: String,
    verifier: String,
}
fn check_proof(attempt: &Attempt, payload: &Proof, origin: &Option<String>) -> bool {
    let challenge: [u8; 32] = Sha256::digest(payload.verifier.as_bytes()).into();
    canonical(&payload.verifier).is_some()
        && bool::from(attempt.challenge.ct_eq(&challenge))
        && (attempt.platform != Platform::Web || &attempt.origin == origin)
}
async fn take(
    State(broker): State<Arc<Broker>>,
    headers: HeaderMap,
    payload: Result<Json<Proof>, JsonRejection>,
) -> Response {
    let origin = match broker.origin(&headers) {
        Ok(value) => value,
        Err(status) => return error(status),
    };
    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(rejection) => return input_error(rejection.status()),
    };
    if canonical(&payload.request_id).is_none() {
        return error(StatusCode::BAD_REQUEST);
    }
    let mut store = broker.store.lock().expect("Apple email store");
    store.prune((broker.clock)());
    let Some(attempt) = store.attempts.get(&payload.request_id) else {
        return error(StatusCode::GONE);
    };
    if !check_proof(attempt, &payload, &origin) {
        return error(StatusCode::FORBIDDEN);
    }
    match &attempt.phase {
        Phase::Pending | Phase::Verifying => json(Outcome::Pending),
        Phase::Ready(outcome) => {
            let result = outcome.clone();
            let state = attempt.state.clone();
            store.attempts.remove(&payload.request_id);
            store.states.remove(&state);
            json(result)
        }
    }
}
async fn cancel(
    State(broker): State<Arc<Broker>>,
    headers: HeaderMap,
    payload: Result<Json<Proof>, JsonRejection>,
) -> Response {
    let origin = match broker.origin(&headers) {
        Ok(value) => value,
        Err(status) => return error(status),
    };
    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(rejection) => return input_error(rejection.status()),
    };
    if canonical(&payload.request_id).is_none() {
        return error(StatusCode::BAD_REQUEST);
    }
    let mut store = broker.store.lock().expect("Apple email store");
    store.prune((broker.clock)());
    let Some(attempt) = store.attempts.get_mut(&payload.request_id) else {
        return error(StatusCode::GONE);
    };
    if !check_proof(attempt, &payload, &origin) {
        return error(StatusCode::FORBIDDEN);
    }
    // The proof holder may discard any unconsumed email, including a completed result.
    attempt.phase = Phase::Ready(Outcome::Cancelled);
    json(Outcome::Cancelled)
}
#[derive(Deserialize)]
struct Callback {
    state: String,
    code: Option<String>,
    id_token: Option<String>,
    error: Option<String>,
    user: Option<String>,
}
async fn callback(
    State(broker): State<Arc<Broker>>,
    payload: Result<Form<Callback>, FormRejection>,
) -> Response {
    let payload = match payload {
        Ok(Form(payload)) => payload,
        Err(rejection) => return input_error(rejection.status()),
    };
    let _ = payload.user; // Apple's unsigned user object is never an email authority.
    let work = {
        let mut store = broker.store.lock().expect("Apple email store");
        store.prune((broker.clock)());
        let Some(id) = store.states.get(&payload.state).cloned() else {
            return error(StatusCode::BAD_REQUEST);
        };
        let Some(attempt) = store.attempts.get_mut(&id) else {
            return error(StatusCode::BAD_REQUEST);
        };
        if !matches!(attempt.phase, Phase::Pending) {
            return error(StatusCode::BAD_REQUEST);
        }
        attempt.phase = Phase::Verifying;
        (id, attempt.nonce.clone(), attempt.created)
    };
    let outcome = if let Some(provider_error) = payload.error {
        if provider_error == "user_cancelled_authorize" {
            Outcome::Cancelled
        } else {
            Outcome::Error {
                code: "invalid_response",
            }
        }
    } else if let (Some(token), Some(code)) = (payload.id_token, payload.code) {
        if token.len() > 16 * 1024 || code.len() > 2048 {
            Outcome::Error {
                code: "invalid_response",
            }
        } else {
            match broker.verifier.verify(&token, &code, &work.1, work.2).await {
                Ok(email) => Outcome::Success { email },
                Err(()) => Outcome::Error {
                    code: "invalid_response",
                },
            }
        }
    } else {
        Outcome::Error {
            code: "invalid_response",
        }
    };
    let (platform, origin) = {
        let mut store = broker.store.lock().expect("Apple email store");
        store.prune((broker.clock)());
        let Some(attempt) = store.attempts.get_mut(&work.0) else {
            return error(StatusCode::GONE);
        };
        if !matches!(attempt.phase, Phase::Verifying) {
            return error(StatusCode::CONFLICT);
        }
        attempt.phase = Phase::Ready(outcome);
        (attempt.platform, attempt.origin.clone())
    };
    let script_nonce = random();
    let mut response = if platform == Platform::Android {
        (
            StatusCode::SEE_OTHER,
            [(
                header::LOCATION,
                format!("{ANDROID_RETURN}?requestId={}", work.0),
            )],
        )
            .into_response()
    } else {
        let target = serde_json::to_string(&origin.expect("Web origin")).expect("origin JSON");
        Html(format!("<!doctype html><meta charset=utf-8><title>MetaSecret</title><script nonce=\"{script_nonce}\">window.opener?.postMessage({{type:'APPLE_EMAIL_RESULT',requestId:'{}'}},{target});window.close()</script><p>Return to MetaSecret.</p>", work.0)).into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_str(&format!("default-src 'none'; script-src 'nonce-{script_nonce}'; base-uri 'none'; frame-ancestors 'none'")).expect("random CSP nonce"));
    response
}
