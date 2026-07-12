//! `POST /_/api/presign` — mint a query-string SigV4 presigned URL for
//! `{method, bucket, key, expires_in_s}`, signed server-side with the configured
//! credentials and the **request host** (so the URL resolves against this
//! instance). See [`crate::presign`] for the signing itself.

use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::header::HOST;
use hyper::{Response, StatusCode};
use s3s::Body;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::http::AppState;
use crate::presign::{presigned_url, PresignInput};

/// One week — S3's maximum presigned-URL lifetime.
const MAX_EXPIRES_S: u64 = 604_800;

#[derive(Deserialize)]
struct PresignRequest {
    method: String,
    bucket: String,
    key: String,
    expires_in_s: u64,
}

#[derive(Serialize)]
struct PresignResponse {
    url: String,
    expires_at: String,
}

pub async fn presign(req: hyper::Request<Incoming>, state: &Arc<AppState>) -> Response<Body> {
    // The signed host must equal the host the browser will hit.
    let host = req
        .headers()
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.to_owned())
        .unwrap_or_else(|| format!("{}:{}", state.bind, state.port));

    let body = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => return super::error(StatusCode::BAD_REQUEST, "InvalidRequest", e.to_string()),
    };
    let parsed: PresignRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return super::error(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                format!("invalid presign request body: {e}"),
            )
        }
    };

    let method = parsed.method.to_ascii_uppercase();
    if method != "GET" && method != "PUT" {
        return super::error(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "method must be GET or PUT",
        );
    }
    if parsed.expires_in_s == 0 || parsed.expires_in_s > MAX_EXPIRES_S {
        return super::error(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            format!("expires_in_s must be 1..={MAX_EXPIRES_S}"),
        );
    }

    let now = OffsetDateTime::now_utc();
    let amz_date = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    );

    let url = presigned_url(&PresignInput {
        method: &method,
        host: &host,
        bucket: &parsed.bucket,
        key: &parsed.key,
        access_key: &state.access_key,
        secret_key: &state.secret_key,
        region: &state.region,
        expires_in_s: parsed.expires_in_s,
        amz_date: &amz_date,
    });

    let expires_at = (now + Duration::seconds(parsed.expires_in_s as i64))
        .format(&Rfc3339)
        .unwrap_or_default();

    super::json(StatusCode::OK, &PresignResponse { url, expires_at })
}
