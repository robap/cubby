//! `GET /_/api/health` — liveness plus the chrome the UI's top bar and nav
//! footer display (data-dir, endpoint, region, and bucket/object counts).

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::header::HOST;
use hyper::{Response, StatusCode};
use s3s::Body;
use serde::Serialize;

use crate::http::AppState;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    uptime_s: u64,
    data_dir: String,
    endpoint: String,
    region: String,
    bucket_count: i64,
    object_count: i64,
}

/// Build the health payload. Counts come from two cheap `COUNT(*)` queries; the
/// endpoint reflects the host the client actually reached (so it is correct
/// even under `--port 0` or behind Docker), falling back to the bind address.
pub fn health(state: &Arc<AppState>, req: &hyper::Request<Incoming>) -> Response<Body> {
    let bucket_count = match state.db.count_buckets() {
        Ok(n) => n,
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };
    let object_count = match state.db.count_objects() {
        Ok(n) => n,
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };

    let host = req
        .headers()
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.to_owned())
        .unwrap_or_else(|| format!("{}:{}", state.bind, state.port));

    let payload = Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_s: state.started_at.elapsed().as_secs(),
        data_dir: state.datadir.root().display().to_string(),
        endpoint: format!("http://{host}"),
        region: state.region.clone(),
        bucket_count,
        object_count,
    };
    super::json(StatusCode::OK, &payload)
}
