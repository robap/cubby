//! The thin JSON/SSE seam under `/_/api/` that backs the web UI.
//!
//! These handlers reuse `Store`/`Db`/`DataDir` **directly** — no self-HTTP, no
//! SigV4 in the UI path (per CONCEPT: keep the seam thin and swappable). Every
//! response is JSON (or SSE); an unknown API path is a JSON `404`, never the
//! SPA `index.html` (that split is what keeps client-side deep links working).

mod buckets;
mod events;
mod health;
mod objects;
mod presign;
mod search;

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::header::CONTENT_TYPE;
use hyper::{Method, Response, StatusCode};
use s3s::Body;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::http::AppState;

/// Route one `/_/api/*` request to its handler. The full request is passed so
/// handlers that need the body (upload) or headers (host for presign/health)
/// can reach them.
pub async fn dispatch(req: hyper::Request<Incoming>, state: &Arc<AppState>) -> Response<Body> {
    let method = req.method().clone();
    // Everything after `/_/api`, without the leading slash: `health`,
    // `buckets/demo/objects`, … An empty tail is the bare `/_/api` path.
    let sub = req
        .uri()
        .path()
        .trim_start_matches("/_/api")
        .trim_start_matches('/')
        .to_owned();

    match (&method, sub.as_str()) {
        (&Method::GET, "health") => health::health(state, &req),
        (&Method::GET, "events") => events::stream(&req, state),
        (&Method::GET, "buckets") => buckets::list(state),
        (&Method::POST, "buckets") => buckets::create(req, state).await,
        (&Method::GET, "search") => search::search(&req, state),
        (&Method::POST, "presign") => presign::presign(req, state).await,
        _ => match parse_objects_path(&sub) {
            Some((bucket, key)) => objects::route(method, req, state, bucket, key).await,
            None => error(
                StatusCode::NOT_FOUND,
                "NotFound",
                format!("no such API endpoint: {method} /_/api/{sub}"),
            ),
        },
    }
}

/// Parse a `buckets/{bucket}/objects[/{key}]` path tail into its bucket and
/// (percent-decoded) key. The bucket has no `/`, so the first `/objects`
/// literal is always the separator. Returns `None` for non-object paths.
fn parse_objects_path(sub: &str) -> Option<(String, Option<String>)> {
    let rest = sub.strip_prefix("buckets/")?;
    let (bucket, tail) = match rest.split_once("/objects") {
        Some((b, tail)) if !b.is_empty() => (b, tail),
        _ => return None,
    };
    // `tail` is "" (list) or "/{key}" (a specific object); an empty key is a
    // list too.
    let key = tail
        .strip_prefix('/')
        .filter(|k| !k.is_empty())
        .map(pct_decode);
    Some((bucket.to_owned(), key))
}

/// Percent-decode a path segment (keys are percent-encoded in URLs).
pub(crate) fn pct_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

/// First value of query parameter `key`, percent-decoded (`+` → space).
pub(crate) fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    let q = query?;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return Some(pct_decode(&v.replace('+', " ")));
        }
    }
    None
}

/// Whether query parameter `key` is present (with any value, or none).
pub(crate) fn query_has_key(query: Option<&str>, key: &str) -> bool {
    query.is_some_and(|q| {
        q.split('&')
            .any(|pair| pair.split_once('=').map(|(k, _)| k).unwrap_or(pair) == key)
    })
}

/// Format Unix seconds as an ISO-8601 (RFC 3339) UTC string for the JSON seam.
pub(crate) fn iso8601(secs: i64) -> String {
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_default()
}

/// Serialize `value` as a JSON response with the given status.
pub(crate) fn json<T: Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    match serde_json::to_vec(value) {
        Ok(bytes) => Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(Body::from(bytes))
            .expect("json response builds"),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            format!("failed to serialize response: {e}"),
        ),
    }
}

/// The seam's consistent error envelope: `{"error":{"code","message"}}`.
pub(crate) fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response<Body> {
    #[derive(Serialize)]
    struct Envelope<'a> {
        error: Inner<'a>,
    }
    #[derive(Serialize)]
    struct Inner<'a> {
        code: &'a str,
        message: String,
    }
    let body = serde_json::to_vec(&Envelope {
        error: Inner {
            code,
            message: message.into(),
        },
    })
    .expect("error envelope serializes");
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .body(Body::from(body))
        .expect("error response builds")
}
