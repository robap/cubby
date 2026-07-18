//! Read-only per-bucket CORS display under `GET /_/api/buckets/{bucket}/cors`.
//!
//! This seam makes a bucket's CORS config **visible** in the web UI without
//! duplicating the management surface: creating/editing/deleting CORS stays the
//! real S3 API (`PutBucketCors`/`DeleteBucketCors`) — the fidelity point. It
//! reads the same `bucket_cors` SQLite table the enforcement path does, so the
//! display reflects the live config with no restart. Any non-GET method is
//! `405` (there is no write endpoint here, by design).

use std::sync::Arc;

use hyper::{Method, Response, StatusCode};
use s3s::Body;
use serde::Serialize;

use crate::http::AppState;

/// Dispatch by method: `GET` returns the bucket's rules (or `null`); anything
/// else is `405` (management is the S3 API, not this seam).
pub fn route(method: Method, state: &Arc<AppState>, bucket: String) -> Response<Body> {
    match method {
        Method::GET => get(state, &bucket),
        m => super::error(
            StatusCode::METHOD_NOT_ALLOWED,
            "MethodNotAllowed",
            format!("{m} not allowed on cors (management is the S3 API)"),
        ),
    }
}

/// `{ "cors": [...] | null }` — the bucket's rules, or `null` when it has none
/// (the "no CORS configured" empty state the UI renders).
#[derive(Serialize)]
struct CorsResponse {
    cors: Option<Vec<crate::cors::CorsRule>>,
}

/// `GET …/cors` — read the stored rules from the same table the enforcement path
/// uses.
fn get(state: &Arc<AppState>, bucket: &str) -> Response<Body> {
    match state.db.get_bucket_cors(bucket) {
        Ok(Some(json)) => match crate::cors::parse_rules(&json) {
            Ok(rules) => super::json(StatusCode::OK, &CorsResponse { cors: Some(rules) }),
            Err(e) => super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            ),
        },
        Ok(None) => super::json(StatusCode::OK, &CorsResponse { cors: None }),
        Err(e) => super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        ),
    }
}
