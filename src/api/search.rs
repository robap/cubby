//! `GET /_/api/search?q=&bucket=&max-keys=` — flat substring key search.
//!
//! A **UI/seam convenience, not an S3 capability**: it substring-matches full
//! keys (`key LIKE '%q%'`), never rolls up folders, is optionally scoped to one
//! bucket (else global across all buckets), and is capped with a `truncated`
//! flag. Distinct from the live-log's own event filter.

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::{Response, StatusCode};
use s3s::Body;
use serde::Serialize;

use crate::http::AppState;

/// Default and hard cap on returned matches.
const DEFAULT_MAX_KEYS: usize = 1000;
const MAX_MAX_KEYS: usize = 10_000;

#[derive(Serialize)]
struct SearchResponse {
    q: String,
    bucket: Option<String>,
    results: Vec<Hit>,
    truncated: bool,
}

#[derive(Serialize)]
struct Hit {
    bucket: String,
    key: String,
    size: i64,
    etag: String,
    last_modified: String,
}

pub fn search(req: &hyper::Request<Incoming>, state: &Arc<AppState>) -> Response<Body> {
    let query = req.uri().query();
    let q = super::query_param(query, "q").unwrap_or_default();
    let bucket = super::query_param(query, "bucket").filter(|s| !s.is_empty());
    let max_keys = super::query_param(query, "max-keys")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_KEYS)
        .min(MAX_MAX_KEYS);

    // An empty query matches nothing (the UI clears search to return to the
    // folder view rather than dumping every key).
    if q.is_empty() {
        return super::json(
            StatusCode::OK,
            &SearchResponse {
                q,
                bucket,
                results: Vec::new(),
                truncated: false,
            },
        );
    }

    // Fetch one extra row to detect truncation without a second query.
    let rows = match state
        .db
        .search_objects(bucket.as_deref(), &q, max_keys as i64 + 1)
    {
        Ok(rows) => rows,
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };
    let truncated = rows.len() > max_keys;
    let results = rows
        .into_iter()
        .take(max_keys)
        .map(|r| Hit {
            bucket: r.bucket,
            key: r.key,
            size: r.size,
            etag: r.etag,
            last_modified: super::iso8601(r.last_modified),
        })
        .collect();

    super::json(
        StatusCode::OK,
        &SearchResponse {
            q,
            bucket,
            results,
            truncated,
        },
    )
}
