//! `GET /_/api/buckets` — the bucket list with per-bucket object count and
//! total size (the browser's bucket column) — and `POST /_/api/buckets`, the
//! UI's create-bucket action (the S3 `CreateBucket` verb behind the seam).

use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{Response, StatusCode};
use s3s::Body;
use serde::{Deserialize, Serialize};

use crate::http::AppState;
use crate::store::unix_now;

#[derive(Serialize)]
struct BucketsResponse {
    buckets: Vec<BucketJson>,
}

#[derive(Serialize)]
struct BucketJson {
    name: String,
    created_at: String,
    object_count: i64,
    size: i64,
}

/// List all buckets with their stats, in name order.
pub fn list(state: &Arc<AppState>) -> Response<Body> {
    let rows = match state.db.list_buckets_with_stats() {
        Ok(rows) => rows,
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };
    let buckets = rows
        .into_iter()
        .map(|b| BucketJson {
            name: b.name,
            created_at: super::iso8601(b.created_at),
            object_count: b.object_count,
            size: b.total_size,
        })
        .collect();
    super::json(StatusCode::OK, &BucketsResponse { buckets })
}

#[derive(Deserialize)]
struct CreateRequest {
    name: String,
}

#[derive(Serialize)]
struct CreatedResponse {
    name: String,
}

/// `POST /_/api/buckets` `{"name":"…"}` — create a bucket. Mirrors the S3
/// `CreateBucket` path exactly: validate the name, make the directory **first**,
/// then insert the row (a crash between leaves an orphan dir that reads as
/// "does not exist"). `409` if it already exists, `400` for an invalid name.
pub async fn create(req: hyper::Request<Incoming>, state: &Arc<AppState>) -> Response<Body> {
    let bytes = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => return super::error(StatusCode::BAD_REQUEST, "InvalidRequest", e.to_string()),
    };
    let parsed: CreateRequest = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return super::error(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                format!("invalid JSON body: {e}"),
            )
        }
    };
    let name = parsed.name.trim();
    if !is_valid_bucket_name(name) {
        return super::error(
            StatusCode::BAD_REQUEST,
            "InvalidBucketName",
            "bucket name must be 3–63 chars: lowercase letters, digits, '-' or \
             '.', starting and ending alphanumeric",
        );
    }

    // Directory first, then the row (crash-ordering invariant, per CONCEPT).
    let dir = state.datadir.bucket_dir(name);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        );
    }
    match state.db.create_bucket(name, unix_now()) {
        Ok(true) => super::json(
            StatusCode::OK,
            &CreatedResponse {
                name: name.to_owned(),
            },
        ),
        Ok(false) => super::error(
            StatusCode::CONFLICT,
            "BucketAlreadyExists",
            format!("bucket already exists: {name}"),
        ),
        Err(e) => super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        ),
    }
}

/// S3-flavored bucket-name check: 3–63 chars, lowercase alphanumerics plus `-`
/// and `.`, first/last character alphanumeric. Rejects uppercase, `_`, and `/`
/// (so the seam can't mint a name the S3 layer would refuse, and `_`-prefixed
/// names that would shadow `/_/` never occur).
fn is_valid_bucket_name(name: &str) -> bool {
    let len = name.len();
    if !(3..=63).contains(&len) {
        return false;
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
    {
        return false;
    }
    let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let bytes = name.as_bytes();
    alnum(bytes[0]) && alnum(bytes[len - 1])
}

#[cfg(test)]
mod tests {
    use super::is_valid_bucket_name;

    #[test]
    fn accepts_conventional_names() {
        assert!(is_valid_bucket_name("demo"));
        assert!(is_valid_bucket_name("app-assets"));
        assert!(is_valid_bucket_name("logs.2026"));
    }

    #[test]
    fn rejects_bad_names() {
        assert!(!is_valid_bucket_name("ab")); // too short
        assert!(!is_valid_bucket_name(&"a".repeat(64))); // too long
        assert!(!is_valid_bucket_name("_private")); // underscore / leading special
        assert!(!is_valid_bucket_name("Demo")); // uppercase
        assert!(!is_valid_bucket_name("a/b")); // slash
        assert!(!is_valid_bucket_name("-lead")); // leading hyphen
        assert!(!is_valid_bucket_name("trail.")); // trailing dot
    }
}
