use super::apple_email::{Broker, Config, EmailVerifier};
use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tower::ServiceExt;

struct TestVerifier;
#[async_trait]
impl EmailVerifier for TestVerifier {
    async fn verify(
        &self,
        token: &str,
        _code: &str,
        _nonce: &str,
        _created: u64,
    ) -> Result<String, ()> {
        if token == "synthetic-valid" {
            Ok("Owner@privaterelay.appleid.com".into())
        } else {
            Err(())
        }
    }
}
fn setup() -> (Arc<Broker>, Arc<AtomicU64>) {
    let clock = Arc::new(AtomicU64::new(1_000));
    let copy = clock.clone();
    let config = Config::parse(
        "test.apple.service",
        "https://broker.example.test/apple/callback",
        "https://meta-secret.github.io,http://localhost:5174",
    )
    .unwrap();
    (
        Broker::new(
            config,
            Arc::new(TestVerifier),
            Arc::new(move || copy.load(Ordering::SeqCst)),
        ),
        clock,
    )
}
async fn request(broker: &Arc<Broker>, path: &str, body: Value) -> (StatusCode, Value) {
    let response = broker
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .header("origin", "http://localhost:5174")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 32_768).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn verifier() -> String {
    "A".repeat(43)
}
async fn start(broker: &Arc<Broker>, platform: &str) -> Value {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier().as_bytes()));
    let (status, body) = request(
        broker,
        "/apple/start",
        json!({"platform": platform, "challenge": challenge}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body
}
async fn callback(broker: &Arc<Broker>, state: &str, token: &str) -> (StatusCode, String) {
    let response = broker
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/apple/callback")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "state={state}&code=synthetic-code&id_token={token}"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 32_768).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
#[tokio::test]
async fn apple_email_start_fixes_provider_and_return_contract() {
    let (broker, _) = setup();
    let body = start(&broker, "web").await;
    assert_eq!(body["requestId"].as_str().unwrap().len(), 43);
    assert_eq!(body["expiresAt"], 1300);
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    assert_eq!(
        url.origin().ascii_serialization(),
        "https://appleid.apple.com"
    );
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(params["response_mode"], "form_post");
    assert_eq!(params["response_type"], "code id_token");
    assert_eq!(params["scope"], "email");
    assert_eq!(
        params["redirect_uri"],
        "https://broker.example.test/apple/callback"
    );
    assert!(!url.as_str().contains(&verifier()));
}
#[tokio::test]
async fn apple_email_callback_then_proof_bound_single_use_take() {
    let (broker, _) = setup();
    let body = start(&broker, "web").await;
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let (status, html) = callback(&broker, &state, "synthetic-valid").await;
    assert_eq!(status, StatusCode::OK);
    for secret in ["synthetic-valid", "synthetic-code", "Owner@", &verifier()] {
        assert!(!html.contains(secret));
    }
    let bad = request(
        &broker,
        "/apple/take",
        json!({"requestId":body["requestId"],"verifier":"B".repeat(43)}),
    )
    .await;
    assert_eq!(bad.0, StatusCode::FORBIDDEN);
    let payload = json!({"requestId":body["requestId"],"verifier":verifier()});
    let result = request(&broker, "/apple/take", payload.clone()).await;
    assert_eq!(
        result,
        (
            StatusCode::OK,
            json!({"status":"success","email":"Owner@privaterelay.appleid.com"})
        )
    );
    assert_eq!(
        request(&broker, "/apple/take", payload).await.0,
        StatusCode::GONE
    );
    assert_eq!(
        callback(&broker, &state, "synthetic-valid").await.0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn apple_email_cancel_is_idempotent_and_rejects_late_callback() {
    let (broker, _) = setup();
    let body = start(&broker, "android").await;
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let payload = json!({"requestId":body["requestId"],"verifier":verifier()});
    for _ in 0..2 {
        assert_eq!(
            request(&broker, "/apple/cancel", payload.clone()).await.1,
            json!({"status":"cancelled"})
        );
    }
    assert_eq!(
        callback(&broker, &state, "synthetic-valid").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&broker, "/apple/take", payload).await.1,
        json!({"status":"cancelled"})
    );
}
#[tokio::test]
async fn apple_email_pending_does_not_consume_and_ttl_expires() {
    let (broker, clock) = setup();
    let body = start(&broker, "web").await;
    let payload = json!({"requestId":body["requestId"],"verifier":verifier()});
    for _ in 0..2 {
        assert_eq!(
            request(&broker, "/apple/take", payload.clone()).await.1,
            json!({"status":"pending"})
        );
    }
    clock.store(1300, Ordering::SeqCst);
    assert_eq!(
        request(&broker, "/apple/take", payload).await.0,
        StatusCode::GONE
    );
}
#[tokio::test]
async fn apple_email_rejects_unknown_state_bad_json_and_foreign_origin() {
    let (broker, _) = setup();
    assert_eq!(
        callback(&broker, "A", "synthetic-valid").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &broker,
            "/apple/start",
            json!({"platform":"web","challenge":"bad","redirect":"https://evil.test"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let response = broker
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/apple/start")
                .header("content-type", "application/json")
                .header("origin", "https://evil.test")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}
#[test]
fn apple_email_config_rejects_unregistered_or_insecure_destinations() {
    for url in [
        "http://broker.test/apple/callback",
        "https://localhost/apple/callback",
        "https://127.0.0.1/apple/callback",
        "https://broker.test/apple/callback?redirect=bad",
        "https://broker.test/other",
    ] {
        assert!(
            Config::parse("service", url, "https://meta-secret.github.io").is_err(),
            "{url}"
        );
    }
}

#[test]
fn apple_email_proof_uses_published_cross_platform_ascii_vector() {
    let verifier = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        "6oZqdX5MOLq_qBJ8vppAnT4fk6AP8UiP9zX8-Rev_9A"
    );
}
#[tokio::test]
async fn apple_email_terminal_take_is_atomic_under_concurrency() {
    let (broker, _) = setup();
    let body = start(&broker, "web").await;
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        callback(&broker, &state, "synthetic-valid").await.0,
        StatusCode::OK
    );
    let payload = json!({"requestId":body["requestId"],"verifier":verifier()});
    let (one, two) = tokio::join!(
        request(&broker, "/apple/take", payload.clone()),
        request(&broker, "/apple/take", payload)
    );
    let mut statuses = [one.0.as_u16(), two.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 410]);
}
#[tokio::test]
async fn apple_email_cancel_drops_already_verified_email_before_take() {
    let (broker, _) = setup();
    let body = start(&broker, "web").await;
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        callback(&broker, &state, "synthetic-valid").await.0,
        StatusCode::OK
    );
    let payload = json!({"requestId":body["requestId"],"verifier":verifier()});
    assert_eq!(
        request(&broker, "/apple/cancel", payload.clone()).await.0,
        StatusCode::OK
    );
    assert_eq!(
        request(&broker, "/apple/take", payload).await.1,
        json!({"status":"cancelled"})
    );
}
#[tokio::test]
async fn apple_email_cancel_while_verifying_fences_late_success() {
    struct Held {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    #[async_trait]
    impl EmailVerifier for Held {
        async fn verify(
            &self,
            _token: &str,
            _code: &str,
            _nonce: &str,
            _created: u64,
        ) -> Result<String, ()> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok("late@example.test".into())
        }
    }
    let held = Arc::new(Held {
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let config = Config::parse(
        "service",
        "https://broker.example.test/apple/callback",
        "http://localhost:5174",
    )
    .unwrap();
    let broker = Broker::new(config, held.clone(), Arc::new(|| 1000));
    let body = start(&broker, "web").await;
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let copy = broker.clone();
    let job = tokio::spawn(async move { callback(&copy, &state, "held").await });
    held.entered.notified().await;
    let payload = json!({"requestId":body["requestId"],"verifier":verifier()});
    assert_eq!(
        request(&broker, "/apple/cancel", payload.clone()).await.0,
        StatusCode::OK
    );
    held.release.notify_one();
    assert_eq!(job.await.unwrap().0, StatusCode::CONFLICT);
    assert_eq!(
        request(&broker, "/apple/take", payload).await.1,
        json!({"status":"cancelled"})
    );
}
#[tokio::test]
async fn apple_email_callback_csp_matches_executable_script_nonce_and_fixed_destination() {
    let (broker, _) = setup();
    let body = start(&broker, "web").await;
    let url = reqwest::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let response = broker
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/apple/callback")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "state={state}&code=synthetic-code&id_token=synthetic-valid"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    let csp = response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(!csp.contains("unsafe-inline"));
    assert!(
        response
            .headers()
            .get("cross-origin-opener-policy")
            .is_none()
    );
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let html = String::from_utf8(
        to_bytes(response.into_body(), 32768)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let nonce = html
        .split("nonce=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert_eq!(nonce.len(), 43);
    assert!(csp.contains(&format!("'nonce-{nonce}'")));
    assert!(html.contains("\"http://localhost:5174\""));
    assert!(html.contains("APPLE_EMAIL_RESULT"));
    assert!(!html.contains("synthetic-valid"));
    assert!(!html.contains("Owner@"));
    // Optional non-secret fixture for the existing Web JS runtime smoke check.
    if let Ok(path) = std::env::var("APPLE_CALLBACK_TEST_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec(&json!({
                "html": html, "csp": csp, "requestId": body["requestId"],
                "targetOrigin": "http://localhost:5174"
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
#[tokio::test]
async fn apple_email_http_rejects_duplicate_fields_oversized_and_wrong_types() {
    let (broker, _) = setup();
    for (path, content, body, expected) in [
        (
            "/apple/start",
            "application/json",
            "{\"platform\":\"web\",\"platform\":\"android\",\"challenge\":\"bad\"}".into(),
            400,
        ),
        ("/apple/start", "text/plain", "{}".into(), 415),
        ("/apple/start", "application/json", "x".repeat(20000), 413),
        (
            "/apple/callback",
            "application/x-www-form-urlencoded",
            "state=x&state=y".into(),
            400,
        ),
        (
            "/apple/callback",
            "application/x-www-form-urlencoded",
            "x".repeat(40000),
            413,
        ),
    ] {
        let response = broker
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", content)
                    .header("origin", "http://localhost:5174")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected, "{path}");
    }
}
