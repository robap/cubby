//! Routing front-door tests: the `.well-known` probe filter and the bare-root
//! browser redirect (spec areas B & D). Driven with `reqwest`, the way the
//! acceptance criteria probe the server with `curl`.

mod common;

use common::TestServer;
use cubby::events::{BusSignal, Event};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::time::{timeout, Duration};

fn base(server: &TestServer) -> String {
    format!("http://{}", server.addr)
}

/// A reqwest client that does not follow redirects, so a `302` is observable.
fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// Drain events until `stop` matches (returning every event seen up to and
/// including it), tolerating lag. Panics on timeout.
async fn drain_until(rx: &mut Receiver<BusSignal>, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        match timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Ok(BusSignal::Event(ev))) => {
                let done = stop(&ev);
                seen.push(ev);
                if done {
                    return seen;
                }
            }
            Ok(Ok(BusSignal::Clear)) => continue,
            Ok(Err(RecvError::Lagged(_))) => continue,
            Ok(Err(e)) => panic!("event stream closed: {e}"),
            Err(_) => panic!("timed out; events so far: {seen:?}"),
        }
    }
}

// B1 — browser probes (`.well-known` discovery and `/favicon.ico`) return 404
// and never enter the request log, while ordinary S3 traffic still appears.
#[tokio::test]
async fn browser_probes_are_filtered_from_the_log() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);
    let http = reqwest::Client::new();

    for path in [
        "/.well-known/appspecific/com.chrome.devtools.json",
        "/favicon.ico",
    ] {
        let resp = http
            .get(format!("{}{path}", base(&server)))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404, "{path} should 404");
    }

    // A signed CreateBucket right after is the marker we drain up to.
    server
        .client()
        .create_bucket()
        .bucket("demo")
        .send()
        .await
        .unwrap();
    let seen = drain_until(&mut rx, |e| e.op.as_deref() == Some("CreateBucket")).await;
    assert!(
        seen.iter().all(|e| !matches!(
            e.bucket.as_deref(),
            Some(".well-known") | Some("favicon.ico")
        )),
        "no browser-probe event should be logged: {seen:?}"
    );
}

// D4 — a browser-shaped bare `GET /` (Accept: text/html, no SigV4) redirects to
// the UI and logs no event.
#[tokio::test]
async fn browser_root_redirects_to_ui_without_logging() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);
    let http = no_redirect_client();

    let resp = http
        .get(format!("{}/", base(&server)))
        .header("Accept", "text/html")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 302);
    assert_eq!(resp.headers().get("location").unwrap(), "/_/");

    // Marker: a signed request that *does* log, proving the redirect did not.
    server
        .client()
        .create_bucket()
        .bucket("demo")
        .send()
        .await
        .unwrap();
    let seen = drain_until(&mut rx, |e| e.op.as_deref() == Some("CreateBucket")).await;
    assert!(
        seen.iter()
            .all(|e| !(e.method == "GET" && e.bucket.is_none())),
        "the redirect must not appear as a root/ListBuckets event: {seen:?}"
    );
}

// D5 — a non-HTML bare `GET /` still reaches the S3 handler (not redirected),
// and a signed ListBuckets returns the bucket list.
#[tokio::test]
async fn root_without_html_accept_falls_through_to_s3() {
    let server = TestServer::spawn().await;
    let http = no_redirect_client();

    // No `Accept: text/html` → not a browser → not redirected; reaches S3.
    let resp = http
        .get(format!("{}/", base(&server)))
        .send()
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        302,
        "a non-browser root request must not be redirected"
    );

    // A signed ListBuckets still returns the list (no redirect on the S3 path).
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    let out = s3.list_buckets().send().await.unwrap();
    let names: Vec<_> = out.buckets().iter().filter_map(|b| b.name()).collect();
    assert!(
        names.contains(&"demo"),
        "ListBuckets returns demo: {names:?}"
    );
}
