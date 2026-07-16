//! HTTP layer: the routing skeleton, the request-log capture wrapper, and the
//! hyper serve loop.
//!
//! One port, routed: `/_/*` is the web UI (its embedded assets + the `/_/api/*`
//! JSON/SSE seam); everything else is S3 wire traffic handed to the `s3s`
//! service, which owns the S3 wire protocol, header SigV4, and XML.
//! Underscore-prefixed bucket names are illegal in S3, so there is zero
//! namespace collision.
//!
//! Every S3 request flows through [`log_and_serve_s3`], the authoritative
//! emitter of the live-request log: it times the request, counts request/
//! response bytes, reads the final status, and — joined through a request-
//! extension slot the [`crate::access_log::AccessLog`] hook fills — records the
//! resolved operation and auth kind (see `access_log.rs`).

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::{Body as HttpBody, Frame, Incoming, SizeHint};
use hyper::header::CONTENT_LENGTH;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use s3s::auth::SimpleAuth;
use s3s::service::{S3Service, S3ServiceBuilder};
use s3s::{HttpError, HttpRequest, HttpResponse};
use tokio::net::TcpListener;

use crate::access_log::{AccessLog, CaptureSlot, SharedSlot};
use crate::banner;
use crate::datadir::DataDir;
use crate::db::Db;
use crate::embed;
use crate::events::{Auth, EventBus, EventDraft};
use crate::store::Store;

/// Everything needed to build and run the server.
pub struct ServeConfig {
    pub bind: String,
    pub port: u16,
    pub access_key: String,
    pub secret_key: String,
    pub datadir: DataDir,
    pub db: Db,
    /// The live-request event bus (SSE + stdout + ndjson feed).
    pub events: EventBus,
    /// Suppress the pretty per-request stdout line (`--quiet`, for CI).
    pub quiet: bool,
    /// Optional seed file (`--seed`): applied before the port binds so a
    /// malformed fixture fails fast without ever looking like a running server.
    pub seed: Option<std::path::PathBuf>,
}

/// Shared state the web UI's JSON/SSE seam reads directly. Cheap to clone
/// behind an `Arc`; held by the [`Router`] and handed to `/_/api/*` handlers.
pub struct AppState {
    pub db: Db,
    pub datadir: DataDir,
    pub bind: String,
    pub port: u16,
    /// Credentials the API's presign endpoint signs URLs with (the same pair
    /// `s3s`/`SimpleAuth` validates).
    pub access_key: String,
    pub secret_key: String,
    /// The accepted-and-ignored default region, for display in the UI chrome.
    pub region: String,
    /// Process start, for the health payload's `uptime_s`.
    pub started_at: Instant,
    /// The live-request event bus.
    pub events: EventBus,
    /// Whether the pretty stdout line is suppressed.
    pub quiet: bool,
}

/// The top routing layer wrapping the `s3s` service.
#[derive(Clone)]
pub struct Router {
    s3: S3Service,
    state: Arc<AppState>,
}

impl hyper::service::Service<hyper::Request<Incoming>> for Router {
    type Response = HttpResponse;
    type Error = HttpError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: hyper::Request<Incoming>) -> Self::Future {
        // `/_/` is the reserved web-UI namespace (underscore-prefixed bucket
        // names are illegal in S3, so there is no collision). Everything else
        // is S3 wire traffic — captured by the request log, then handed to the
        // `s3s` service.
        let path = req.uri().path();
        if path == "/_" || path == "/_/" || path.starts_with("/_/") {
            let state = self.state.clone();
            return Box::pin(async move { Ok(serve_ui(req, state).await) });
        }
        // Browser probes like `/.well-known/appspecific/com.chrome.devtools.json`
        // and `/favicon.ico` are never S3 traffic; short-circuit them with a `404`
        // before the log/S3 path so they never emit an event or a pretty stdout
        // line. (The web UI's own tab icon is an inline data-URI in `index.html`.)
        if is_browser_probe(path) {
            return Box::pin(async { Ok(empty_status(hyper::StatusCode::NOT_FOUND)) });
        }
        // A bare `GET /` from a human's browser (asks for HTML, no SigV4) is the
        // documented front door → redirect to the UI. This is a pure routing
        // decision: no event is logged. Everything else at `/` (a signed
        // ListBuckets, or any non-HTML request) falls through to the S3 handler.
        if req.method() == hyper::Method::GET
            && path == "/"
            && looks_like_browser(req.headers(), req.uri().query())
        {
            return Box::pin(async { Ok(redirect_to_ui()) });
        }
        let s3 = self.s3.clone();
        let state = self.state.clone();
        Box::pin(log_and_serve_s3(s3, state, req))
    }
}

pin_project_lite::pin_project! {
    /// A pass-through body that counts the data bytes flowing through it into a
    /// shared counter — used to measure a request's `bytes_in` without buffering
    /// (the log must never add latency or hold whole objects in memory).
    struct CountingBody<B> {
        #[pin]
        inner: B,
        counter: Arc<AtomicU64>,
    }
}

impl<B> HttpBody for CountingBody<B>
where
    B: HttpBody<Data = Bytes>,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let polled = this.inner.poll_frame(cx);
        if let std::task::Poll::Ready(Some(Ok(frame))) = &polled {
            if let Some(data) = frame.data_ref() {
                this.counter.fetch_add(data.len() as u64, Ordering::Relaxed);
            }
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

/// Serve one S3 request while capturing it into the live-request log. Wraps the
/// request body to count `bytes_in`, inserts the capture slot the access hook
/// fills (resolved op + auth), times the call, then emits one [`Event`].
///
/// [`Event`]: crate::events::Event
async fn log_and_serve_s3(
    s3: S3Service,
    state: Arc<AppState>,
    req: hyper::Request<Incoming>,
) -> Result<HttpResponse, HttpError> {
    let method = req.method().as_str().to_owned();
    let (bucket, key) = parse_bucket_key(req.uri().path());

    let bytes_in = Arc::new(AtomicU64::new(0));
    let slot: SharedSlot = Arc::new(Mutex::new(CaptureSlot::default()));

    // Rebuild the request with (a) the capture slot in its extensions — which
    // survive into the `S3AccessContext` — and (b) a byte-counting body.
    let (mut parts, incoming) = req.into_parts();
    parts.extensions.insert(slot.clone());
    let counted = CountingBody {
        inner: incoming,
        counter: bytes_in.clone(),
    };
    let s3_req: HttpRequest = HttpRequest::from_parts(parts, s3s::Body::http_body(counted));

    let start = Instant::now();
    let result = s3.call(s3_req).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    // A transport-level error (rare — op errors are serialized to responses)
    // still gets logged as a 500 before propagating.
    let response = match result {
        Ok(resp) => resp,
        Err(err) => {
            emit_event(
                &state,
                EventDraft {
                    method,
                    op: None,
                    bucket,
                    key,
                    status: 500,
                    duration_ms,
                    bytes_in: bytes_in.load(Ordering::Relaxed),
                    bytes_out: 0,
                    auth: Auth::Anonymous,
                    error_code: None,
                },
            );
            return Err(err);
        }
    };

    let status = response.status().as_u16();
    let bytes_out = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let (op, auth) = {
        let slot = slot.lock().expect("capture slot poisoned");
        (slot.op.clone(), slot.auth.unwrap_or(Auth::Anonymous))
    };

    // On failures, surface the S3 `<Code>` (e.g. `NoSuchKey`,
    // `SignatureDoesNotMatch`) so the log answers "why did it 403" at a glance.
    // Error bodies are small XML, so buffering one to parse it is safe — success
    // bodies (object bytes) are never touched.
    let (response, error_code) = if status >= 400 {
        let (parts, body) = response.into_parts();
        match body.collect().await {
            Ok(collected) => {
                let bytes = collected.to_bytes();
                let code = parse_error_code(&bytes);
                (
                    HttpResponse::from_parts(parts, s3s::Body::from(bytes)),
                    code,
                )
            }
            // Couldn't read the body — log without a code rather than fail.
            Err(_) => (HttpResponse::from_parts(parts, s3s::Body::empty()), None),
        }
    } else {
        (response, None)
    };

    emit_event(
        &state,
        EventDraft {
            method,
            op,
            bucket,
            key,
            status,
            duration_ms,
            bytes_in: bytes_in.load(Ordering::Relaxed),
            bytes_out,
            auth,
            error_code,
        },
    );

    Ok(response)
}

/// Extract the S3 error code from an error response body: the text of the first
/// `<Code>…</Code>` element in the XML.
fn parse_error_code(xml: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(xml).ok()?;
    let start = text.find("<Code>")? + "<Code>".len();
    let end = text[start..].find("</Code>")? + start;
    Some(text[start..end].to_owned())
}

/// Record the event on the bus and print the pretty stdout line unless quiet.
fn emit_event(state: &AppState, draft: EventDraft) {
    let event = state.events.emit(draft);
    if !state.quiet {
        println!("{}", crate::events::pretty(&event));
    }
}

/// Derive `(bucket, key)` from a path-style request path for the log. The first
/// segment is the bucket, the remainder the key; both are percent-decoded for
/// display. `/` (ListBuckets) yields `(None, None)`.
fn parse_bucket_key(path: &str) -> (Option<String>, Option<String>) {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return (None, None);
    }
    // The bucket is always the first path segment; the key is the remainder (a
    // trailing slash, as in `PUT /demo/`, means bucket-only, no key).
    match trimmed.split_once('/') {
        Some((b, k)) => (Some(pct_decode(b)), (!k.is_empty()).then(|| pct_decode(k))),
        None => (Some(pct_decode(trimmed)), None),
    }
}

/// Lossy percent-decode for display in the log.
fn pct_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

/// Whether a path is browser-probe noise that is never real S3 traffic — a
/// `/.well-known/…` discovery probe or the `/favicon.ico` tab-icon request.
/// Matched at the root so a bucket key that merely embeds the literal deeper in
/// its path is unaffected.
fn is_browser_probe(path: &str) -> bool {
    path == "/favicon.ico" || path.starts_with("/.well-known/")
}

/// Whether a bare-root request is a human's browser: it asks for HTML
/// (`Accept: text/html`) and presents no SigV4 auth (no `Authorization` header,
/// no `X-Amz-*` query params). A real S3 client never asks for HTML, so keying
/// on `Accept` stays correct even under accept-any-credentials mode.
fn looks_like_browser(headers: &hyper::HeaderMap, query: Option<&str>) -> bool {
    let accepts_html = headers
        .get(hyper::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"));
    if !accepts_html || headers.contains_key(hyper::header::AUTHORIZATION) {
        return false;
    }
    let has_sigv4_query = query.is_some_and(|q| {
        q.split('&')
            .any(|pair| pair.split('=').next().unwrap_or(pair).starts_with("X-Amz-"))
    });
    !has_sigv4_query
}

/// An empty-body response with the given status (browser-probe short-circuit).
fn empty_status(status: hyper::StatusCode) -> HttpResponse {
    hyper::Response::builder()
        .status(status)
        .body(s3s::Body::empty())
        .expect("empty status response builds")
}

/// A `302` redirect to the web UI (the bare-root browser front door).
fn redirect_to_ui() -> HttpResponse {
    hyper::Response::builder()
        .status(hyper::StatusCode::FOUND)
        .header(hyper::header::LOCATION, "/_/")
        .body(s3s::Body::empty())
        .expect("redirect response builds")
}

/// Serve a `/_/…` request: the JSON/SSE API, a real embedded asset, or the SPA
/// shell (`index.html`) as the fallback for client-side routes.
async fn serve_ui(req: hyper::Request<Incoming>, state: Arc<AppState>) -> HttpResponse {
    let path = req.uri().path();
    // `rel` is the asset path relative to `web/dist/`. `/_` and `/_/` → "".
    let rel = path.strip_prefix("/_/").unwrap_or("");

    // The API namespace is always JSON/SSE and never falls back to index.html.
    if rel == "api" || rel.starts_with("api/") {
        return crate::api::dispatch(req, &state).await;
    }
    if rel.is_empty() {
        return embed::serve_index();
    }
    if let Some(resp) = embed::serve_embedded(rel) {
        return resp;
    }
    // Non-API, non-asset `/_/…` path → the SPA shell (client-side routing).
    embed::serve_index()
}

/// Build the routed service (S3 backend + fixed-credential SigV4 auth + the
/// request-log access hook).
pub fn build_router(cfg: &ServeConfig) -> Router {
    let store = Store::new(cfg.db.clone(), cfg.datadir.clone(), cfg.access_key.clone());
    let mut builder = S3ServiceBuilder::new(store);
    builder.set_auth(SimpleAuth::from_single(
        cfg.access_key.clone(),
        cfg.secret_key.clone(),
    ));
    // The access hook enriches each in-flight event with the resolved op + auth
    // kind. It preserves the default signature-required policy (see
    // `access_log.rs`), so wiring it changes logging, not authorization.
    builder.set_access(AccessLog);
    let state = Arc::new(AppState {
        db: cfg.db.clone(),
        datadir: cfg.datadir.clone(),
        bind: cfg.bind.clone(),
        port: cfg.port,
        access_key: cfg.access_key.clone(),
        secret_key: cfg.secret_key.clone(),
        region: "us-east-1".to_owned(),
        started_at: Instant::now(),
        events: cfg.events.clone(),
        quiet: cfg.quiet,
    });
    Router {
        s3: builder.build(),
        state,
    }
}

/// Accept connections forever, serving each with the router. Returns only on a
/// fatal accept error.
pub async fn run_accept_loop(listener: TcpListener, router: Router) -> std::io::Result<()> {
    loop {
        let (stream, _peer) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let router = router.clone();
        tokio::spawn(async move {
            if let Err(err) = ConnBuilder::new(TokioExecutor::new())
                .serve_connection(io, router)
                .await
            {
                tracing::debug!("connection error: {err}");
            }
        });
    }
}

/// Bind, print the banner, and serve until a fatal error. Used by `main`.
pub async fn serve(cfg: ServeConfig) -> anyhow::Result<()> {
    // Apply the seed (if any) *before* binding: a malformed fixture must fail
    // fast with a non-zero exit and nothing listening, never a running server
    // in a half-known state. The banner prints only once seeding succeeds.
    if let Some(seed_path) = &cfg.seed {
        let store = Store::new(cfg.db.clone(), cfg.datadir.clone(), cfg.access_key.clone());
        crate::seed::apply(seed_path, &store).await?;
    }

    let listener = TcpListener::bind((cfg.bind.as_str(), cfg.port)).await?;
    let addr: SocketAddr = listener.local_addr()?;
    banner::print(addr, &cfg.access_key, &cfg.secret_key, cfg.datadir.root());
    let router = build_router(&cfg);
    run_accept_loop(listener, router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bucket_key_splits_path_style() {
        assert_eq!(parse_bucket_key("/"), (None, None));
        assert_eq!(parse_bucket_key("/demo"), (Some("demo".to_owned()), None));
        // A trailing slash (CreateBucket: `PUT /demo/`) is bucket-only.
        assert_eq!(parse_bucket_key("/demo/"), (Some("demo".to_owned()), None));
        assert_eq!(
            parse_bucket_key("/demo/a/b.txt"),
            (Some("demo".to_owned()), Some("a/b.txt".to_owned()))
        );
        // Keys are percent-decoded for display.
        assert_eq!(
            parse_bucket_key("/demo/a%20b.txt"),
            (Some("demo".to_owned()), Some("a b.txt".to_owned()))
        );
    }

    #[test]
    fn parse_error_code_reads_the_code_element() {
        let xml = br#"<?xml version="1.0"?><Error><Code>SignatureDoesNotMatch</Code><Message>x</Message></Error>"#;
        assert_eq!(
            parse_error_code(xml).as_deref(),
            Some("SignatureDoesNotMatch")
        );
        assert_eq!(parse_error_code(b"not xml"), None);
    }

    #[test]
    fn is_browser_probe_matches_well_known_and_favicon() {
        assert!(is_browser_probe(
            "/.well-known/appspecific/com.chrome.devtools.json"
        ));
        assert!(is_browser_probe("/.well-known/security.txt"));
        // The browser's tab-icon probe — never S3 traffic.
        assert!(is_browser_probe("/favicon.ico"));
        // Not a probe: the root, the UI namespace, or a bucket that merely
        // happens to embed the literal deeper in a key/path.
        assert!(!is_browser_probe("/"));
        assert!(!is_browser_probe("/_/browser"));
        assert!(!is_browser_probe("/demo/.well-known/x"));
        assert!(!is_browser_probe("/demo/favicon.ico"));
    }

    #[test]
    fn looks_like_browser_requires_html_and_no_sigv4() {
        use hyper::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};

        let mut html = HeaderMap::new();
        html.insert(
            ACCEPT,
            HeaderValue::from_static("text/html,application/xhtml+xml"),
        );
        assert!(looks_like_browser(&html, None));
        // A harmless (non-SigV4) query does not disqualify a browser.
        assert!(looks_like_browser(&html, Some("foo=bar")));

        // No `Accept: text/html` → a real S3 client, never redirected.
        assert!(!looks_like_browser(&HeaderMap::new(), None));

        // Header SigV4 → signed, not a bare browser hit.
        let mut signed = html.clone();
        signed.insert(
            AUTHORIZATION,
            HeaderValue::from_static("AWS4-HMAC-SHA256 Credential=local/..."),
        );
        assert!(!looks_like_browser(&signed, None));

        // Presigned (query SigV4) → not redirected either.
        assert!(!looks_like_browser(
            &html,
            Some("X-Amz-Signature=abc&X-Amz-Date=x")
        ));
    }
}
