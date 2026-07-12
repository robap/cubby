//! `GET /_/api/events` — the live request log as Server-Sent Events, and its
//! `?format=ndjson` twin (newline-delimited JSON for `jq`/test harnesses).
//!
//! Both replay the ring buffer first (honoring `Last-Event-ID` for reconnect
//! replay), then stream live. A consumer that lags the broadcast channel
//! receives a synthetic `dropped` marker rather than stalling the server.

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::StreamBody;
use hyper::body::{Frame, Incoming};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::{Response, StatusCode};
use s3s::Body;
use tokio::sync::broadcast::error::RecvError;

use crate::events::Event;
use crate::http::AppState;

/// Stream the event log. SSE by default; newline-delimited JSON when
/// `?format=ndjson` is set.
pub fn stream(req: &hyper::Request<Incoming>, state: &Arc<AppState>) -> Response<Body> {
    let ndjson = req
        .uri()
        .query()
        .map(|q| query_has(q, "format", "ndjson"))
        .unwrap_or(false);

    // Reconnect replay: `Last-Event-ID` resumes just after the given id.
    let after = req
        .headers()
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());

    let (backlog, mut rx) = state.events.subscribe(after);

    let body_stream = async_stream::stream! {
        for ev in backlog {
            yield Ok::<_, Infallible>(Frame::data(frame_bytes(&ev, ndjson)));
        }
        loop {
            match rx.recv().await {
                Ok(ev) => yield Ok(Frame::data(frame_bytes(&ev, ndjson))),
                Err(RecvError::Lagged(n)) => yield Ok(Frame::data(dropped_bytes(n, ndjson))),
                Err(RecvError::Closed) => break,
            }
        }
    };

    let content_type = if ndjson {
        "application/x-ndjson; charset=utf-8"
    } else {
        "text/event-stream; charset=utf-8"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header(CACHE_CONTROL, "no-cache")
        // Defeat proxy buffering so events arrive promptly.
        .header("x-accel-buffering", "no")
        .body(Body::http_body(StreamBody::new(body_stream)))
        .expect("event stream response builds")
}

/// Encode one event as either an SSE frame (`id:`+`data:`) or an ndjson line.
fn frame_bytes(ev: &Event, ndjson: bool) -> Bytes {
    let json = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_owned());
    if ndjson {
        Bytes::from(format!("{json}\n"))
    } else {
        Bytes::from(format!("id: {}\ndata: {json}\n\n", ev.id))
    }
}

/// Encode a "dropped N events" marker for a lagged consumer.
fn dropped_bytes(n: u64, ndjson: bool) -> Bytes {
    if ndjson {
        Bytes::from(format!("{{\"dropped\":{n}}}\n"))
    } else {
        Bytes::from(format!("event: dropped\ndata: {{\"dropped\":{n}}}\n\n"))
    }
}

/// Whether a URL query string contains `key=value` (minimal, no full parse).
fn query_has(query: &str, key: &str, value: &str) -> bool {
    query
        .split('&')
        .any(|pair| pair.split_once('=') == Some((key, value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Auth;

    fn ev() -> Event {
        Event {
            id: 7,
            ts: 0,
            method: "PUT".to_owned(),
            op: Some("PutObject".to_owned()),
            bucket: Some("demo".to_owned()),
            key: Some("k".to_owned()),
            status: 200,
            duration_ms: 3,
            bytes_in: 4,
            bytes_out: 0,
            auth: Auth::Header,
            error_code: None,
        }
    }

    #[test]
    fn sse_frame_carries_id_and_data() {
        let b = frame_bytes(&ev(), false);
        let s = std::str::from_utf8(&b).unwrap();
        assert!(s.starts_with("id: 7\ndata: {"), "frame: {s:?}");
        assert!(s.ends_with("}\n\n"));
        assert!(s.contains("\"op\":\"PutObject\""));
    }

    #[test]
    fn ndjson_frame_is_one_line_no_sse_framing() {
        let b = frame_bytes(&ev(), true);
        let s = std::str::from_utf8(&b).unwrap();
        assert!(!s.contains("data:"));
        assert!(s.ends_with("}\n"));
        assert_eq!(s.matches('\n').count(), 1);
    }

    #[test]
    fn dropped_marker_shapes() {
        assert_eq!(
            std::str::from_utf8(&dropped_bytes(5, true)).unwrap(),
            "{\"dropped\":5}\n"
        );
        assert_eq!(
            std::str::from_utf8(&dropped_bytes(5, false)).unwrap(),
            "event: dropped\ndata: {\"dropped\":5}\n\n"
        );
    }

    #[test]
    fn query_has_matches_exact_pair() {
        assert!(query_has("format=ndjson", "format", "ndjson"));
        assert!(query_has("a=1&format=ndjson&b=2", "format", "ndjson"));
        assert!(!query_has("format=sse", "format", "ndjson"));
        assert!(!query_has("", "format", "ndjson"));
    }
}
