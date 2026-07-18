//! Per-bucket webhook-notification config under
//! `/_/api/buckets/{bucket}/notifications[/{id}]`: list, create (with write-time
//! validation), and delete a destination.
//!
//! This is the seam that models notification config as **mutable bucket state**
//! (per `docs/features/event-notifications-spec.md`): destinations live in the
//! `bucket_notifications` SQLite table, are created/removed here at runtime, and
//! take effect immediately (the delivery engine reads the table on each
//! mutation). Validation happens **here, at write time** — a bad destination is
//! rejected with `400` and nothing is persisted.

use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{Method, Response, StatusCode};
use s3s::Body;
use serde::{Deserialize, Serialize};

use crate::db::NotificationDraft;
use crate::http::AppState;
use crate::notify::{self, DEFAULT_TIMEOUT_MS};
use crate::store::unix_now;

/// Dispatch by method and whether an `{id}` is present in the path.
pub async fn route(
    method: Method,
    req: hyper::Request<Incoming>,
    state: &Arc<AppState>,
    bucket: String,
    id: Option<String>,
) -> Response<Body> {
    match (method, id) {
        (Method::GET, None) => list(state, &bucket),
        (Method::POST, None) => create(req, state, &bucket).await,
        (Method::DELETE, Some(id)) => delete(state, &bucket, &id).await,
        (Method::DELETE, None) => super::error(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "a notification id is required to delete",
        ),
        (Method::POST, Some(_)) | (Method::GET, Some(_)) => super::error(
            StatusCode::METHOD_NOT_ALLOWED,
            "MethodNotAllowed",
            "method not allowed on a specific notification",
        ),
        (m, _) => super::error(
            StatusCode::METHOD_NOT_ALLOWED,
            "MethodNotAllowed",
            format!("{m} not allowed on notifications"),
        ),
    }
}

/// One destination as the seam returns it (a GET-list entry, and the POST
/// response body). `created_at` is rendered ISO-8601 like the rest of the seam.
#[derive(Serialize)]
struct NotificationJson {
    id: i64,
    url: String,
    events: Vec<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    format: String,
    timeout_ms: i64,
    created_at: String,
}

impl From<crate::db::NotificationRow> for NotificationJson {
    fn from(r: crate::db::NotificationRow) -> Self {
        Self {
            id: r.id,
            url: r.url,
            events: r.events,
            prefix: r.prefix,
            suffix: r.suffix,
            format: r.format,
            timeout_ms: r.timeout_ms,
            created_at: super::iso8601(r.created_at),
        }
    }
}

#[derive(Serialize)]
struct ListResponse {
    notifications: Vec<NotificationJson>,
}

/// `GET …/notifications` — the bucket's destinations, in insertion order.
fn list(state: &Arc<AppState>, bucket: &str) -> Response<Body> {
    match state.db.list_bucket_notifications(bucket) {
        Ok(rows) => super::json(
            StatusCode::OK,
            &ListResponse {
                notifications: rows.into_iter().map(Into::into).collect(),
            },
        ),
        Err(e) => super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        ),
    }
}

/// The POST body: `url` and `events` are required; `prefix`/`suffix`/`format`/
/// `timeout_ms` are optional (format defaults to `s3-notification`, timeout to
/// 5000).
#[derive(Deserialize)]
struct CreateRequest {
    url: String,
    events: Vec<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    suffix: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    timeout_ms: Option<i64>,
}

/// `POST …/notifications` — validate a destination at write time and insert it,
/// returning `201` with the created row (including its `id`). A malformed body
/// or an invalid destination is `400` and persists nothing.
async fn create(
    req: hyper::Request<Incoming>,
    state: &Arc<AppState>,
    bucket: &str,
) -> Response<Body> {
    // The destination is bucket state, so the bucket must exist (the FK would
    // reject it anyway; a 404 is the honest answer).
    match state.db.bucket_exists(bucket) {
        Ok(true) => {}
        Ok(false) => {
            return super::error(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                format!("no such bucket: {bucket}"),
            )
        }
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    }

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

    // Resolve defaults, and treat an empty prefix/suffix as "no constraint".
    let format = parsed
        .format
        .unwrap_or_else(|| "s3-notification".to_owned());
    let timeout_ms = parsed.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let prefix = parsed.prefix.filter(|s| !s.is_empty());
    let suffix = parsed.suffix.filter(|s| !s.is_empty());

    // Write-time validation: a bad destination is a 400 and persists nothing.
    if let Err(msg) = notify::validate_destination(&parsed.url, &parsed.events, &format, timeout_ms)
    {
        return super::error(StatusCode::BAD_REQUEST, "InvalidNotification", msg);
    }

    let draft = NotificationDraft {
        bucket: bucket.to_owned(),
        url: parsed.url,
        events: parsed.events,
        prefix,
        suffix,
        format,
        timeout_ms,
        created_at: unix_now(),
    };
    let id = match state.db.insert_bucket_notification(&draft) {
        Ok(id) => id,
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };

    let created = NotificationJson {
        id,
        url: draft.url,
        events: draft.events,
        prefix: draft.prefix,
        suffix: draft.suffix,
        format: draft.format,
        timeout_ms: draft.timeout_ms,
        created_at: super::iso8601(draft.created_at),
    };
    super::json(StatusCode::CREATED, &created)
}

/// `DELETE …/notifications/{id}` — remove one destination (scoped to `bucket`).
/// `204` on removal, `404` if no such destination, `400` if `id` isn't an
/// integer.
async fn delete(state: &Arc<AppState>, bucket: &str, id: &str) -> Response<Body> {
    let id: i64 = match id.parse() {
        Ok(n) => n,
        Err(_) => {
            return super::error(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                format!("notification id must be an integer: {id}"),
            )
        }
    };
    match state.db.delete_bucket_notification(bucket, id) {
        Ok(true) => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .expect("delete response builds"),
        Ok(false) => super::error(
            StatusCode::NOT_FOUND,
            "NotFound",
            format!("no such notification: {id}"),
        ),
        Err(e) => super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        ),
    }
}
