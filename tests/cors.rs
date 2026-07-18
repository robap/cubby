//! End-to-end tests for the per-bucket S3 CORS config API, driven by the real
//! `aws-sdk-s3` client: `PutBucketCors` / `GetBucketCors` / `DeleteBucketCors`
//! round-trip, the `NoSuchCORSConfiguration` probe, whole-config replace,
//! idempotent delete, restart persistence, and cascade on bucket delete.

mod common;

use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::types::{CorsConfiguration, CorsRule};
use common::TestServer;
use cubby::events::{BusSignal, Event};
use tokio::sync::broadcast::Receiver;
use tokio::time::{timeout, Duration};

/// A representative rule: origin + methods + a `*` allowed-header, an exposed
/// `ETag`, and a max-age.
fn sample_rule() -> CorsRule {
    CorsRule::builder()
        .allowed_origins("http://localhost:3000")
        .allowed_methods("GET")
        .allowed_methods("PUT")
        .allowed_methods("POST")
        .allowed_headers("*")
        .expose_headers("ETag")
        .max_age_seconds(600)
        .build()
        .unwrap()
}

/// Send an `OPTIONS` preflight for `bucket/photos/cat.jpg` with the given origin,
/// requested method, and (optional) requested headers — the raw wire request a
/// browser sends, unsigned.
async fn preflight(
    server: &TestServer,
    origin: &str,
    method: &str,
    request_headers: Option<&str>,
) -> reqwest::Response {
    let url = format!("http://{}/{}/photos/cat.jpg", server.addr, "uploads");
    let mut req = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, url)
        .header("Origin", origin)
        .header("Access-Control-Request-Method", method);
    if let Some(h) = request_headers {
        req = req.header("Access-Control-Request-Headers", h);
    }
    req.send().await.expect("preflight request sends")
}

/// Pull events until one matches `pred` (or time out). Skips `Clear` and
/// tolerates lag.
async fn recv_matching(rx: &mut Receiver<BusSignal>, pred: impl Fn(&Event) -> bool) -> Event {
    loop {
        match timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Ok(BusSignal::Event(ev))) if pred(&ev) => return ev,
            Ok(Ok(_)) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(e)) => panic!("event stream closed: {e}"),
            Err(_) => panic!("timed out waiting for a matching event"),
        }
    }
}

async fn put_sample(server: &TestServer, bucket: &str) {
    let client = server.client();
    client.create_bucket().bucket(bucket).send().await.unwrap();
    let config = CorsConfiguration::builder()
        .cors_rules(sample_rule())
        .build()
        .unwrap();
    client
        .put_bucket_cors()
        .bucket(bucket)
        .cors_configuration(config)
        .send()
        .await
        .expect("put-bucket-cors succeeds");
}

#[tokio::test]
async fn put_then_get_round_trips_the_rules() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;

    let out = server
        .client()
        .get_bucket_cors()
        .bucket("uploads")
        .send()
        .await
        .expect("get-bucket-cors returns the config");
    let rules = out.cors_rules();
    assert_eq!(rules.len(), 1);
    let r = &rules[0];
    assert_eq!(r.allowed_origins(), ["http://localhost:3000"]);
    assert_eq!(r.allowed_methods(), ["GET", "PUT", "POST"]);
    assert_eq!(r.allowed_headers(), ["*"]);
    assert_eq!(r.expose_headers(), ["ETag"]);
    assert_eq!(r.max_age_seconds(), Some(600));
}

#[tokio::test]
async fn get_on_bucket_without_cors_is_no_such_cors_configuration() {
    let server = TestServer::spawn().await;
    server
        .client()
        .create_bucket()
        .bucket("fresh")
        .send()
        .await
        .unwrap();
    let err = server
        .client()
        .get_bucket_cors()
        .bucket("fresh")
        .send()
        .await
        .expect_err("a bucket with no CORS must 404");
    assert_eq!(
        err.code(),
        Some("NoSuchCORSConfiguration"),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn put_replaces_the_whole_config() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;

    // A second put with a different rule replaces (not appends).
    let replacement = CorsConfiguration::builder()
        .cors_rules(
            CorsRule::builder()
                .allowed_origins("*")
                .allowed_methods("GET")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    server
        .client()
        .put_bucket_cors()
        .bucket("uploads")
        .cors_configuration(replacement)
        .send()
        .await
        .unwrap();

    let out = server
        .client()
        .get_bucket_cors()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    assert_eq!(out.cors_rules().len(), 1);
    assert_eq!(out.cors_rules()[0].allowed_origins(), ["*"]);
    assert_eq!(out.cors_rules()[0].allowed_methods(), ["GET"]);
}

#[tokio::test]
async fn put_rejects_a_rule_with_an_unsupported_method() {
    let server = TestServer::spawn().await;
    server
        .client()
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    // `PATCH` is not in the S3 method set — the config is invalid and nothing is
    // persisted. (The SDK enforces ≥1 method client-side, so an unsupported
    // method is the validation case a real client can actually send.)
    let config = CorsConfiguration::builder()
        .cors_rules(
            CorsRule::builder()
                .allowed_origins("http://localhost:3000")
                .allowed_methods("PATCH")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    let err = server
        .client()
        .put_bucket_cors()
        .bucket("uploads")
        .cors_configuration(config)
        .send()
        .await
        .expect_err("an invalid config must be rejected");
    assert_eq!(err.code(), Some("InvalidRequest"), "unexpected: {err:?}");
    // Nothing was stored.
    let get = server
        .client()
        .get_bucket_cors()
        .bucket("uploads")
        .send()
        .await;
    assert_eq!(get.unwrap_err().code(), Some("NoSuchCORSConfiguration"));
}

#[tokio::test]
async fn delete_removes_the_config_and_is_idempotent() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;

    server
        .client()
        .delete_bucket_cors()
        .bucket("uploads")
        .send()
        .await
        .expect("delete-bucket-cors succeeds");
    // Now a get is NoSuchCORSConfiguration again.
    let err = server
        .client()
        .get_bucket_cors()
        .bucket("uploads")
        .send()
        .await
        .expect_err("deleted config → 404");
    assert_eq!(err.code(), Some("NoSuchCORSConfiguration"));
    // A second delete when none exists still succeeds (tolerant).
    server
        .client()
        .delete_bucket_cors()
        .bucket("uploads")
        .send()
        .await
        .expect("a second delete is tolerant");
}

// --- Actual-request CORS headers --------------------------------------------

/// Presign a `GET` URL for `bucket/key` (300s), the real presigned URL a browser
/// would `fetch()`.
async fn presigned_get_url(server: &TestServer, bucket: &str, key: &str) -> String {
    use aws_sdk_s3::presigning::PresigningConfig;
    let req = server
        .client()
        .get_object()
        .bucket(bucket)
        .key(key)
        .presigned(PresigningConfig::expires_in(Duration::from_secs(300)).unwrap())
        .await
        .expect("presign get");
    req.uri().to_string()
}

#[tokio::test]
async fn presigned_get_with_origin_carries_allow_origin_and_expose_headers() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await; // allows localhost:3000 GET, exposes ETag
    server
        .client()
        .put_object()
        .bucket("uploads")
        .key("photos/cat.jpg")
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"hello"))
        .send()
        .await
        .unwrap();

    let url = presigned_get_url(&server, "uploads", "photos/cat.jpg").await;
    let resp = reqwest::Client::new()
        .get(url)
        .header("Origin", "http://localhost:3000")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let h = resp.headers();
    assert_eq!(
        h.get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
    assert!(
        h.get("access-control-expose-headers")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("ETag"),
        "expose-headers must list ETag"
    );
}

#[tokio::test]
async fn cross_origin_error_still_carries_allow_origin() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;
    // A presigned GET for a key that doesn't exist → 404, but a browser must still
    // be able to read the failure, so allow-origin is present.
    let url = presigned_get_url(&server, "uploads", "photos/missing.jpg").await;
    let resp = reqwest::Client::new()
        .get(url)
        .header("Origin", "http://localhost:3000")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
}

#[tokio::test]
async fn no_config_means_no_cors_headers() {
    let server = TestServer::spawn().await;
    // A bucket with an object but no CORS config.
    server
        .client()
        .create_bucket()
        .bucket("plain")
        .send()
        .await
        .unwrap();
    server
        .client()
        .put_object()
        .bucket("plain")
        .key("photos/cat.jpg")
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"hello"))
        .send()
        .await
        .unwrap();

    let url = presigned_get_url(&server, "plain", "photos/cat.jpg").await;
    let resp = reqwest::Client::new()
        .get(url)
        .header("Origin", "http://localhost:3000")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    // No config → no CORS headers, identical to a fresh S3 bucket / cubby today.
    assert!(resp.headers().get("access-control-allow-origin").is_none());
    assert!(resp
        .headers()
        .get("access-control-expose-headers")
        .is_none());
}

// --- Read-only UI seam (GET /_/api/buckets/{bucket}/cors) --------------------

#[tokio::test]
async fn ui_seam_returns_rules_and_null_and_rejects_non_get() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;
    server
        .client()
        .create_bucket()
        .bucket("plain")
        .send()
        .await
        .unwrap();

    let base = format!("http://{}/_/api/buckets", server.addr);
    let client = reqwest::Client::new();

    // A configured bucket returns its rules under `cors`.
    let body: serde_json::Value = client
        .get(format!("{base}/uploads/cors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rules = body["cors"].as_array().expect("cors is an array");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["AllowedOrigins"][0], "http://localhost:3000");
    assert_eq!(rules[0]["ExposeHeaders"][0], "ETag");

    // A bucket with no config returns `cors: null` (the empty state).
    let body: serde_json::Value = client
        .get(format!("{base}/plain/cors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(body["cors"].is_null());

    // The seam is read-only — a non-GET is 405.
    let resp = client
        .delete(format!("{base}/uploads/cors"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 405);
}

// --- Preflight (OPTIONS answered before auth) -------------------------------

#[tokio::test]
async fn preflight_with_matching_rule_returns_204_without_auth() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await; // allows localhost:3000 + GET/PUT/POST, headers *

    let resp = preflight(
        &server,
        "http://localhost:3000",
        "PUT",
        Some("authorization,content-type"),
    )
    .await;

    assert_eq!(resp.status(), 204, "a matching preflight is 204, not 403");
    let h = resp.headers();
    assert_eq!(
        h.get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
    assert!(
        h.get("access-control-allow-methods")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("PUT"),
        "allow-methods must cover PUT"
    );
    // The requested headers are covered (rule allows `*`).
    let allow_headers = h
        .get("access-control-allow-headers")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allow_headers.contains("authorization"));
    assert!(allow_headers.contains("content-type"));
    // A non-zero max-age was configured (600).
    assert_eq!(h.get("access-control-max-age").unwrap(), "600");
}

#[tokio::test]
async fn preflight_from_non_matching_origin_is_403_with_no_allow_origin() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;

    let resp = preflight(&server, "http://evil.test", "PUT", None).await;
    assert_eq!(resp.status(), 403);
    assert!(
        resp.headers().get("access-control-allow-origin").is_none(),
        "a refused preflight carries no allow-origin"
    );
}

#[tokio::test]
async fn preflight_for_non_allowed_method_has_no_allow_origin() {
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    // A rule that allows only GET.
    let config = CorsConfiguration::builder()
        .cors_rules(
            CorsRule::builder()
                .allowed_origins("http://localhost:3000")
                .allowed_methods("GET")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    client
        .put_bucket_cors()
        .bucket("uploads")
        .cors_configuration(config)
        .send()
        .await
        .unwrap();

    // The preflight requests DELETE → refused.
    let resp = preflight(&server, "http://localhost:3000", "DELETE", None).await;
    assert_eq!(resp.status(), 403);
    assert!(resp.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn star_origin_rule_yields_literal_star() {
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    let config = CorsConfiguration::builder()
        .cors_rules(
            CorsRule::builder()
                .allowed_origins("*")
                .allowed_methods("GET")
                .allowed_methods("PUT")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    client
        .put_bucket_cors()
        .bucket("uploads")
        .cors_configuration(config)
        .send()
        .await
        .unwrap();

    let resp = preflight(&server, "http://whatever.test", "PUT", None).await;
    assert_eq!(resp.status(), 204);
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    // A `*` grant does not vary by origin.
    assert!(resp.headers().get("vary").is_none());
}

#[tokio::test]
async fn rejected_preflight_emits_a_live_log_event() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;
    let mut rx = server.events.subscribe(None).1;

    let resp = preflight(&server, "http://evil.test", "PUT", None).await;
    assert_eq!(resp.status(), 403);

    // A synthetic Preflight event names the op, the origin, and that it was
    // rejected — the S3-debugger visibility promise.
    let ev = recv_matching(&mut rx, |e| e.op.as_deref() == Some("Preflight")).await;
    assert_eq!(ev.method, "OPTIONS");
    assert_eq!(ev.status, 403);
    let note = ev.note.unwrap_or_default();
    assert!(note.contains("rejected"), "note: {note}");
    assert!(note.contains("http://evil.test"), "note: {note}");
}

#[tokio::test]
async fn plain_options_without_preflight_headers_falls_through() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;
    // An OPTIONS with no Access-Control-Request-Method is not a preflight; it must
    // not be answered as one (it falls through to s3s — not a 204 CORS grant).
    let url = format!("http://{}/uploads/photos/cat.jpg", server.addr);
    let resp = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, url)
        .header("Origin", "http://localhost:3000")
        .send()
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        204,
        "a non-preflight OPTIONS is not a CORS grant"
    );
    assert!(resp.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn deleting_the_bucket_cascades_its_cors() {
    let server = TestServer::spawn().await;
    put_sample(&server, "uploads").await;

    // Remove the bucket (empty — CORS config isn't an object), then re-create it.
    server
        .client()
        .delete_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    server
        .client()
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    // The re-created same-named bucket starts with no CORS (the cascade removed
    // the old row).
    let err = server
        .client()
        .get_bucket_cors()
        .bucket("uploads")
        .send()
        .await
        .expect_err("re-created bucket has no CORS");
    assert_eq!(err.code(), Some("NoSuchCORSConfiguration"));
}
