//! Object endpoints under `/_/api/buckets/{bucket}/objects[/{key}]`:
//! folder-view listing, object metadata, byte streaming (with `Range`), upload,
//! and delete. These reuse the same storage disciplines as the S3 path — the
//! Phase 2 listing engine, and the temp→fsync→rename→row write and
//! row-first-then-unlink delete orderings from CONCEPT — but speak plain JSON.
//!
//! Per decision #2, UI-originated uploads/deletes are **not** S3 wire traffic
//! and are deliberately **not** logged: they run here, never through the S3
//! logging wrapper.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hyper::body::{Frame, Incoming};
use hyper::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use hyper::{Method, Response, StatusCode};
use md5::{Digest, Md5};
use s3s::Body;
use serde::Serialize;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::db::ObjectRow;
use crate::http::AppState;
use crate::keypath::key_to_relpath;
use crate::listing::{self, ListParams};
use crate::store::unix_now;

/// S3's default content type when none is supplied.
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";
/// Unique-per-process temp file counter (mirrors the S3 write path).
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Dispatch by method and whether a key is present.
pub async fn route(
    method: Method,
    req: hyper::Request<Incoming>,
    state: &Arc<AppState>,
    bucket: String,
    key: Option<String>,
) -> Response<Body> {
    match (method, key) {
        (Method::GET, None) => list(&req, state, &bucket),
        (Method::GET, Some(k)) => {
            if super::query_has_key(req.uri().query(), "content")
                || req.uri().path().ends_with("/content")
            {
                content(&req, state, &bucket, &k).await
            } else {
                meta(state, &bucket, &k)
            }
        }
        (Method::PUT, Some(k)) => upload(req, state, &bucket, &k).await,
        (Method::DELETE, Some(k)) => delete(state, &bucket, &k).await,
        (Method::PUT | Method::DELETE, None) => super::error(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "a key is required",
        ),
        (m, _) => super::error(
            StatusCode::METHOD_NOT_ALLOWED,
            "MethodNotAllowed",
            format!("{m} not allowed on objects"),
        ),
    }
}

// --- Folder-view listing ----------------------------------------------------

#[derive(Serialize)]
struct FolderResponse {
    prefix: String,
    delimiter: Option<String>,
    common_prefixes: Vec<String>,
    objects: Vec<ObjectJson>,
    next_continuation_token: Option<String>,
}

#[derive(Serialize)]
struct ObjectJson {
    key: String,
    size: i64,
    etag: String,
    last_modified: String,
}

/// `GET …/objects?prefix=&delimiter=/&continuation-token=&max-keys=` — one page
/// of the bucket, delimiter-rolled into folders, via the Phase 2 listing engine.
fn list(req: &hyper::Request<Incoming>, state: &Arc<AppState>, bucket: &str) -> Response<Body> {
    let query = req.uri().query();
    let prefix = super::query_param(query, "prefix").unwrap_or_default();
    let delimiter = super::query_param(query, "delimiter").filter(|d| !d.is_empty());
    let max_keys = super::query_param(query, "max-keys")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1000)
        .min(1000);

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

    let start_from = match super::query_param(query, "continuation-token") {
        Some(tok) => match listing::decode_token(&tok) {
            Ok(cursor) => Some(cursor),
            Err(_) => {
                return super::error(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    "invalid continuation-token",
                )
            }
        },
        None => None,
    };

    // Drive the pure listing engine over the SQLite seek primitive, capturing
    // the first DB error rather than unwinding through the engine.
    let mut db_err = None;
    let fetch = |from: Option<&str>, limit: i64| -> Vec<ObjectRow> {
        match state.db.list_objects_page(bucket, &prefix, from, limit) {
            Ok(rows) => rows,
            Err(e) => {
                db_err.get_or_insert(e);
                Vec::new()
            }
        }
    };
    let params = ListParams {
        prefix: &prefix,
        delimiter: delimiter.as_deref(),
        start_from,
        skip_cp_le: None,
        max_keys,
    };
    let page = listing::list_page(fetch, |r: &ObjectRow| r.key.as_str(), &params);
    if let Some(e) = db_err {
        return super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        );
    }

    let objects = page
        .contents
        .into_iter()
        .map(|r| ObjectJson {
            key: r.key,
            size: r.size,
            etag: r.etag,
            last_modified: super::iso8601(r.last_modified),
        })
        .collect();
    let next_continuation_token = page.next_cursor.as_deref().map(listing::encode_token);

    super::json(
        StatusCode::OK,
        &FolderResponse {
            prefix,
            delimiter,
            common_prefixes: page.common_prefixes,
            objects,
            next_continuation_token,
        },
    )
}

// --- Object metadata --------------------------------------------------------

#[derive(Serialize)]
struct MetaResponse {
    key: String,
    size: i64,
    etag: String,
    content_type: Option<String>,
    last_modified: String,
    storage_class: &'static str,
    metadata: serde_json::Value,
}

/// `GET …/objects/{key}` — object metadata, or `404 NoSuchKey`.
fn meta(state: &Arc<AppState>, bucket: &str, key: &str) -> Response<Body> {
    match state.db.get_object(bucket, key) {
        Ok(Some(row)) => {
            let metadata = serde_json::from_str(&row.metadata)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            super::json(
                StatusCode::OK,
                &MetaResponse {
                    key: row.key,
                    size: row.size,
                    etag: row.etag,
                    content_type: row.content_type,
                    last_modified: super::iso8601(row.last_modified),
                    // Storage classes are a CONCEPT non-goal; always STANDARD.
                    storage_class: "STANDARD",
                    metadata,
                },
            )
        }
        Ok(None) => super::error(
            StatusCode::NOT_FOUND,
            "NoSuchKey",
            format!("no such key: {key}"),
        ),
        Err(e) => super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        ),
    }
}

// --- Byte streaming (preview / download) ------------------------------------

/// `GET …/objects/{key}?content` — stream the object's bytes with its stored
/// content-type, honoring a `Range` request (`206` + `Content-Range`).
async fn content(
    req: &hyper::Request<Incoming>,
    state: &Arc<AppState>,
    bucket: &str,
    key: &str,
) -> Response<Body> {
    use futures::StreamExt;
    use http_body_util::StreamBody;
    use tokio::io::AsyncReadExt;
    use tokio_util::io::ReaderStream;

    let row = match state.db.get_object(bucket, key) {
        Ok(Some(row)) => row,
        Ok(None) => {
            return super::error(
                StatusCode::NOT_FOUND,
                "NoSuchKey",
                format!("no such key: {key}"),
            )
        }
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };
    let full_len = row.size as u64;

    // Resolve any Range header against the object length.
    let (offset, length, content_range, status) =
        match req.headers().get(RANGE).and_then(|v| v.to_str().ok()) {
            Some(header) => match parse_range(header, full_len) {
                Some((start, end)) => {
                    let cr = format!("bytes {}-{}/{}", start, end - 1, full_len);
                    (start, end - start, Some(cr), StatusCode::PARTIAL_CONTENT)
                }
                None => {
                    return super::error(
                        StatusCode::RANGE_NOT_SATISFIABLE,
                        "InvalidRange",
                        "requested range not satisfiable",
                    )
                }
            },
            None => (0, full_len, None, StatusCode::OK),
        };

    let path = state.datadir.bucket_dir(bucket).join(key_to_relpath(key));
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            )
        }
    };
    if offset > 0 {
        if let Err(e) = file.seek(std::io::SeekFrom::Start(offset)).await {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            );
        }
    }
    let stream = ReaderStream::new(file.take(length)).map(|r| r.map(Frame::data));
    let body = Body::http_body(StreamBody::new(stream));

    let mut builder = Response::builder()
        .status(status)
        .header(
            CONTENT_TYPE,
            row.content_type.as_deref().unwrap_or(DEFAULT_CONTENT_TYPE),
        )
        .header(CONTENT_LENGTH, length)
        .header(ACCEPT_RANGES, "bytes");
    if let Some(cr) = content_range {
        builder = builder.header(CONTENT_RANGE, cr);
    }
    builder.body(body).expect("content response builds")
}

/// Parse a single-range `bytes=…` header into a half-open `[start, end)` byte
/// range clamped to `full_len`. Returns `None` if unsatisfiable.
fn parse_range(header: &str, full_len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    // Only the first range of a possibly-multi range set is honored.
    let spec = spec.split(',').next()?.trim();
    let (start_s, end_s) = spec.split_once('-')?;
    let range = match (start_s.trim(), end_s.trim()) {
        // Suffix range: last N bytes.
        ("", suffix) => {
            let n: u64 = suffix.parse().ok()?;
            if n == 0 {
                return None;
            }
            let start = full_len.saturating_sub(n);
            (start, full_len)
        }
        (start, "") => {
            let start: u64 = start.parse().ok()?;
            (start, full_len)
        }
        (start, end) => {
            let start: u64 = start.parse().ok()?;
            let end: u64 = end.parse().ok()?;
            // `end` is inclusive in HTTP; make it exclusive and clamp.
            (start, end.saturating_add(1).min(full_len))
        }
    };
    if range.0 >= full_len || range.0 >= range.1 {
        return None;
    }
    Some(range)
}

// --- Upload -----------------------------------------------------------------

#[derive(Serialize)]
struct UploadResponse {
    key: String,
    size: i64,
    etag: String,
}

/// `PUT …/objects/{key}` — stream the request body through the Phase 1 write
/// path (`.tmp/` → fsync → rename → row). Result is a real browsable file.
async fn upload(
    req: hyper::Request<Incoming>,
    state: &Arc<AppState>,
    bucket: &str,
    key: &str,
) -> Response<Body> {
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

    let content_type = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let final_path = state.datadir.bucket_dir(bucket).join(key_to_relpath(key));

    // Stream body → temp file (hashing incrementally, never buffering).
    let (temp_path, size, etag) = match stream_to_temp(state, req.into_body()).await {
        Ok(triple) => triple,
        Err(e) => return super::error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", e),
    };

    // temp → fsync (done) → rename into place → authoritative row last.
    if let Some(parent) = final_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            );
        }
    }
    if let Err(e) = tokio::fs::rename(&temp_path, &final_path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        );
    }

    let row = ObjectRow {
        bucket: bucket.to_owned(),
        key: key.to_owned(),
        size,
        etag: etag.clone(),
        content_type: content_type.or_else(|| Some(DEFAULT_CONTENT_TYPE.to_owned())),
        last_modified: unix_now(),
        metadata: "{}".to_owned(),
    };
    if let Err(e) = state.db.put_object(&row) {
        return super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        );
    }

    super::json(
        StatusCode::OK,
        &UploadResponse {
            key: key.to_owned(),
            size,
            etag,
        },
    )
}

/// Stream a body to a fresh temp file in `.tmp/`, hashing MD5 incrementally
/// (never buffering the whole object), then flush + fsync. Mirrors the S3 write
/// path's `stream_to_temp`. Returns `(temp_path, size, hex_md5)`.
async fn stream_to_temp(
    state: &Arc<AppState>,
    body: Incoming,
) -> Result<(std::path::PathBuf, i64, String), String> {
    use http_body_util::BodyExt;

    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_path = state
        .datadir
        .tmp_dir()
        .join(format!("ui-{}-{n}.tmp", std::process::id()));
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut hasher = Md5::new();
    let mut size: i64 = 0;
    let mut body = body;
    let result: Result<(), String> = async {
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|e| e.to_string())?;
            if let Ok(data) = frame.into_data() {
                hasher.update(&data);
                file.write_all(&data).await.map_err(|e| e.to_string())?;
                size += data.len() as i64;
            }
        }
        file.flush().await.map_err(|e| e.to_string())?;
        file.sync_all().await.map_err(|e| e.to_string())?; // durable before rename
        Ok(())
    }
    .await;

    if let Err(e) = result {
        drop(file);
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(e);
    }
    Ok((temp_path, size, hex::encode(hasher.finalize())))
}

// --- Delete -----------------------------------------------------------------

/// `DELETE …/objects/{key}` — row first (source of truth), then unlink the
/// bytes. Idempotent (`204` whether or not the key existed).
async fn delete(state: &Arc<AppState>, bucket: &str, key: &str) -> Response<Body> {
    if let Err(e) = state.db.delete_object(bucket, key) {
        return super::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            e.to_string(),
        );
    }
    let path: std::path::PathBuf = state.datadir.bucket_dir(bucket).join(key_to_relpath(key));
    if let Err(e) = tokio::fs::remove_file(&path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            return super::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                e.to_string(),
            );
        }
    }
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .expect("delete response builds")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_full_and_partial() {
        assert_eq!(parse_range("bytes=0-4", 10), Some((0, 5)));
        assert_eq!(parse_range("bytes=5-", 10), Some((5, 10)));
        assert_eq!(parse_range("bytes=-3", 10), Some((7, 10)));
        // End past the object is clamped.
        assert_eq!(parse_range("bytes=8-100", 10), Some((8, 10)));
    }

    #[test]
    fn parse_range_rejects_unsatisfiable() {
        assert_eq!(parse_range("bytes=10-20", 10), None); // start >= len
        assert_eq!(parse_range("bytes=abc", 10), None);
        assert_eq!(parse_range("nonsense", 10), None);
        assert_eq!(parse_range("bytes=-0", 10), None);
    }
}
