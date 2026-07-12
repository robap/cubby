//! End-to-end tests for the web UI plumbing (`/_/`) — asset serving, the SPA
//! fallback vs. API-404 split, and the health endpoint. Driven with a plain
//! HTTP client (`reqwest`), the way `curl` exercises the seam in the plan's
//! acceptance criteria.

mod common;

use common::TestServer;

fn base(server: &TestServer) -> String {
    format!("http://{}", server.addr)
}

/// Pull the first `/_/assets/…"` URL out of the served index document.
fn first_asset_url(html: &str) -> String {
    let start = html
        .find("/_/assets/")
        .expect("index references an asset under /_/assets/");
    let rest = &html[start..];
    let end = rest.find(['"', '\'']).expect("asset URL is quoted");
    rest[..end].to_owned()
}

// Box 0.2 — Embed + serve at `/_/`.
#[tokio::test]
async fn index_served_at_underscore_and_501_body_gone() {
    let server = TestServer::spawn().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/_/", base(&server)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(ct.contains("text/html"), "content-type was {ct}");
    let body = resp.text().await.unwrap();
    assert!(body.contains(r#"id="app""#), "index mounts #app: {body}");
    assert!(
        !body.contains("coming in Phase 5"),
        "the 501 placeholder body must be gone"
    );
}

// Box 0.2 — assets load with correct content-types.
#[tokio::test]
async fn referenced_assets_load_200() {
    let server = TestServer::spawn().await;
    let client = reqwest::Client::new();

    let index = client
        .get(format!("{}/_/", base(&server)))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let asset = first_asset_url(&index);

    let resp = client
        .get(format!("{}{}", base(&server), asset))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "asset {asset} should load");
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        ct.contains("javascript") || ct.contains("css"),
        "asset {asset} content-type was {ct}"
    );
}

// Box 0.4 — SPA fallback + API 404 split.
#[tokio::test]
async fn spa_fallback_serves_index_but_api_404s_json() {
    let server = TestServer::spawn().await;
    let client = reqwest::Client::new();

    // A client-side route (not an asset) → the SPA shell.
    let resp = client
        .get(format!("{}/_/buckets/foo", base(&server)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(ct.contains("text/html"), "fallback content-type {ct}");
    assert!(resp.text().await.unwrap().contains(r#"id="app""#));

    // An unknown API path → JSON 404, never HTML.
    let resp = client
        .get(format!("{}/_/api/nope", base(&server)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(ct.contains("application/json"), "404 content-type {ct}");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "NotFound");
}

// Box 0.5 — health.
#[tokio::test]
async fn health_reports_ok_version_and_live_counts() {
    let server = TestServer::spawn().await;
    let client = reqwest::Client::new();
    let url = format!("{}/_/api/health", base(&server));

    let body: serde_json::Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"]
        .as_str()
        .unwrap()
        .chars()
        .next()
        .unwrap()
        .is_ascii_digit());
    assert_eq!(body["region"], "us-east-1");
    assert_eq!(body["bucket_count"], 0);
    assert_eq!(body["object_count"], 0);
    assert!(!body["data_dir"].as_str().unwrap().is_empty());
    assert!(body["endpoint"].as_str().unwrap().starts_with("http://"));

    // Counts track real S3 state.
    server
        .client()
        .create_bucket()
        .bucket("demo")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    assert_eq!(body["bucket_count"], 1);
}
