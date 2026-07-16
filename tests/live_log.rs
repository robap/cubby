//! Live-request-log capture tests: drive the real `aws-sdk-s3` client and
//! assert the event bus recorded the request with its resolved S3 operation,
//! status, bytes, and auth — the correlated-capture mechanism (Group A).

mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::TestServer;
use cubby::events::{Auth, BusSignal, Event};
use futures::StreamExt;
use tokio::sync::broadcast::Receiver;
use tokio::time::{timeout, Duration};

fn base(server: &TestServer) -> String {
    format!("http://{}", server.addr)
}

/// Read a streaming HTTP response into a growing text buffer until `pred` holds
/// (or time out). Used to observe SSE/ndjson frames the way `curl -N` does.
async fn read_until(resp: reqwest::Response, pred: impl Fn(&str) -> bool) -> String {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let sleep = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => panic!("timed out; buffer so far: {buf:?}"),
            chunk = stream.next() => match chunk {
                Some(Ok(bytes)) => {
                    buf.push_str(&String::from_utf8_lossy(&bytes));
                    if pred(&buf) { return buf; }
                }
                Some(Err(e)) => panic!("stream error: {e}"),
                None => panic!("stream ended; buffer: {buf:?}"),
            }
        }
    }
}

/// Pull events until one matches `pred` (or time out). Tolerates `Lagged` and
/// skips `Clear` signals.
async fn recv_matching(rx: &mut Receiver<BusSignal>, pred: impl Fn(&Event) -> bool) -> Event {
    let deadline = Duration::from_secs(5);
    loop {
        match timeout(deadline, rx.recv()).await {
            Ok(Ok(BusSignal::Event(ev))) => {
                if pred(&ev) {
                    return ev;
                }
            }
            Ok(Ok(BusSignal::Clear)) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(e)) => panic!("event stream closed: {e}"),
            Err(_) => panic!("timed out waiting for a matching event"),
        }
    }
}

// A1 — a real PutObject is captured with the resolved op, status, bytes, auth.
#[tokio::test]
async fn put_object_is_captured_with_resolved_op() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);
    let client = server.client();

    client.create_bucket().bucket("demo").send().await.unwrap();
    client
        .put_object()
        .bucket("demo")
        .key("k")
        .body(ByteStream::from_static(b"hello world"))
        .send()
        .await
        .unwrap();

    let ev = recv_matching(&mut rx, |e| e.op.as_deref() == Some("PutObject")).await;
    assert_eq!(ev.method, "PUT");
    assert_eq!(ev.status, 200);
    assert_eq!(ev.bucket.as_deref(), Some("demo"));
    assert_eq!(ev.key.as_deref(), Some("k"));
    assert_eq!(ev.auth, Auth::Header);
    // Wire bytes include the 11-byte payload (plus any chunk framing).
    assert!(ev.bytes_in >= 11, "bytes_in was {}", ev.bytes_in);
    assert!(ev.id > 0);
}

// A1 — the resolved op distinguishes operations sharing a method/path shape.
#[tokio::test]
async fn create_bucket_and_get_object_resolve_distinct_ops() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);
    let client = server.client();

    client.create_bucket().bucket("demo").send().await.unwrap();
    let create = recv_matching(&mut rx, |e| e.op.as_deref() == Some("CreateBucket")).await;
    assert_eq!(create.status, 200);
    assert_eq!(create.bucket.as_deref(), Some("demo"));

    client
        .put_object()
        .bucket("demo")
        .key("obj")
        .body(ByteStream::from_static(b"data"))
        .send()
        .await
        .unwrap();
    let _ = client
        .get_object()
        .bucket("demo")
        .key("obj")
        .send()
        .await
        .unwrap();

    let get = recv_matching(&mut rx, |e| e.op.as_deref() == Some("GetObject")).await;
    assert_eq!(get.method, "GET");
    assert_eq!(get.status, 200);
    assert_eq!(get.key.as_deref(), Some("obj"));
    // GET streams the object back: bytes_out is its size.
    assert_eq!(get.bytes_out, 4);
}

// Box A.5 — a bad signature is logged as 403 with the parsed error code.
#[tokio::test]
async fn bad_signature_is_logged_as_403_with_error_code() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);

    // A signed request with the wrong secret → the signature check fails before
    // the op is resolved, so op is None but the error code is captured.
    let bad = server.client_with("local", "WRONG-SECRET");
    let _ = bad.list_buckets().send().await;

    let ev = recv_matching(&mut rx, |e| e.status == 403).await;
    assert_eq!(ev.error_code.as_deref(), Some("SignatureDoesNotMatch"));
    assert_eq!(ev.op, None, "bad-signature 403 resolves no op");
}

// Box A.5 — an anonymous request is logged as 403 AccessDenied *with* its op
// (the access hook resolves and records the op before denying).
#[tokio::test]
async fn anonymous_request_is_logged_as_403_access_denied_with_op() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);
    let client = reqwest::Client::new();

    // No SigV4 at all → anonymous.
    let _ = client
        .get(format!("{}/demo", base(&server)))
        .send()
        .await
        .unwrap();

    let ev = recv_matching(&mut rx, |e| e.status == 403).await;
    assert_eq!(ev.error_code.as_deref(), Some("AccessDenied"));
    assert_eq!(ev.auth, Auth::Anonymous);
    assert!(ev.op.is_some(), "anonymous request still resolves an op");
}

// Box A.3 — SSE emits live over HTTP with a monotonic `id:`.
#[tokio::test]
async fn sse_emits_live_frame_for_a_request() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();

    // Open the SSE stream first (so the event is delivered live, not replayed).
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{}/_/api/events", base(&server)))
        .send()
        .await
        .unwrap();
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/event-stream"));

    s3.put_object()
        .bucket("demo")
        .key("sse-key")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();

    let buf = read_until(resp, |b| b.contains("PutObject") && b.contains("sse-key")).await;
    assert!(buf.contains("id: "), "SSE frame carries an id: {buf}");
    assert!(buf.contains("data: "), "SSE frame carries data: {buf}");
}

// Box A.4 — ndjson: one JSON object per line, no SSE framing.
#[tokio::test]
async fn ndjson_emits_one_json_object_per_line() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{}/_/api/events?format=ndjson", base(&server)))
        .send()
        .await
        .unwrap();
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("ndjson"));

    s3.put_object()
        .bucket("demo")
        .key("nd-key")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();

    let buf = read_until(resp, |b| b.contains("nd-key")).await;
    // Find the line mentioning our key; it must be a bare JSON object.
    let line = buf
        .lines()
        .find(|l| l.contains("nd-key"))
        .expect("a line for nd-key");
    assert!(!line.contains("data:"), "no SSE framing: {line}");
    let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
    assert_eq!(v["op"], "PutObject");
}

// Replay after reconnect — Last-Event-ID replays only later events.
#[tokio::test]
async fn last_event_id_replays_only_newer_events() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    for k in ["a", "b", "c"] {
        s3.put_object()
            .bucket("demo")
            .key(k)
            .body(ByteStream::from_static(b"x"))
            .send()
            .await
            .unwrap();
    }

    // Snapshot current ids from the ring via a plain (no-header) ndjson read.
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{}/_/api/events?format=ndjson", base(&server)))
        .send()
        .await
        .unwrap();
    let buf = read_until(resp, |b| b.matches("PutObject").count() >= 3).await;
    let ids: Vec<u64> = buf
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["id"].as_u64())
        .collect();
    let cutoff = ids[ids.len() - 2]; // replay should start strictly after this

    let resp = http
        .get(format!("{}/_/api/events?format=ndjson", base(&server)))
        .header("Last-Event-ID", cutoff.to_string())
        .send()
        .await
        .unwrap();
    let buf = read_until(resp, |b| !b.trim().is_empty()).await;
    let replayed: Vec<u64> = buf
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["id"].as_u64())
        .collect();
    assert!(
        replayed.iter().all(|&id| id > cutoff),
        "replayed ids {replayed:?} must all be > {cutoff}"
    );
    assert!(replayed.contains(&(cutoff + 1)), "replay includes cutoff+1");
}

// Box B.clear — `POST /_/api/events/clear` drains the ring and pushes a `clear`
// frame to a connected live stream.
#[tokio::test]
async fn clear_endpoint_drains_ring_and_signals_live_stream() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    s3.put_object()
        .bucket("demo")
        .key("before-clear")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();

    // Open a live ndjson stream (it replays the backlog, then streams live).
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{}/_/api/events?format=ndjson", base(&server)))
        .send()
        .await
        .unwrap();

    // Clearing pushes a `{"clear":true}` frame to the connected stream…
    let clear = http
        .post(format!("{}/_/api/events/clear", base(&server)))
        .send()
        .await
        .unwrap();
    assert_eq!(clear.status(), 204);
    let buf = read_until(resp, |b| b.contains("\"clear\":true")).await;
    assert!(
        buf.contains("before-clear"),
        "backlog replayed first: {buf}"
    );

    // …and a fresh subscriber sees an empty backlog (ring drained).
    let resp2 = http
        .get(format!("{}/_/api/events?format=ndjson", base(&server)))
        .send()
        .await
        .unwrap();
    s3.put_object()
        .bucket("demo")
        .key("after-clear")
        .body(ByteStream::from_static(b"y"))
        .send()
        .await
        .unwrap();
    let buf2 = read_until(resp2, |b| b.contains("after-clear")).await;
    assert!(
        !buf2.contains("before-clear"),
        "pre-clear events are gone from the ring: {buf2}"
    );
}

// THE headline (seam substance) — a multipart upload decomposes into the three
// resolved ops on the event stream: CreateMultipartUpload → UploadPart(s) →
// CompleteMultipartUpload, each `200` with byte counts. The browser renders
// these live from this same stream (the human end-to-end demo is the plan's
// acceptance box); this proves the decomposition the UI depends on.
#[tokio::test]
async fn multipart_upload_decomposes_into_resolved_op_events() {
    let server = TestServer::spawn().await;
    let (_backlog, mut rx) = server.events.subscribe(None);
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();

    let create = s3
        .create_multipart_upload()
        .bucket("demo")
        .key("big.bin")
        .send()
        .await
        .unwrap();
    let upload_id = create.upload_id().unwrap().to_owned();

    // Two tiny parts (the 5MiB floor is intentionally not enforced).
    let mut parts = Vec::new();
    for part_number in 1..=2i32 {
        let up = s3
            .upload_part()
            .bucket("demo")
            .key("big.bin")
            .upload_id(&upload_id)
            .part_number(part_number)
            .body(ByteStream::from(vec![part_number as u8; 8]))
            .send()
            .await
            .unwrap();
        parts.push(
            aws_sdk_s3::types::CompletedPart::builder()
                .part_number(part_number)
                .e_tag(up.e_tag().unwrap())
                .build(),
        );
    }
    s3.complete_multipart_upload()
        .bucket("demo")
        .key("big.bin")
        .upload_id(&upload_id)
        .multipart_upload(
            aws_sdk_s3::types::CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build(),
        )
        .send()
        .await
        .unwrap();

    // The three ops arrive in order, each resolved and 200. `recv_matching`
    // walks the stream forward, so asserting them in sequence is sound.
    let create_ev = recv_matching(&mut rx, |e| {
        e.op.as_deref() == Some("CreateMultipartUpload")
    })
    .await;
    assert_eq!(create_ev.status, 200);
    assert_eq!(create_ev.key.as_deref(), Some("big.bin"));

    let part_ev = recv_matching(&mut rx, |e| e.op.as_deref() == Some("UploadPart")).await;
    assert_eq!(part_ev.status, 200);
    assert!(
        part_ev.bytes_in >= 8,
        "part carries its bytes: {}",
        part_ev.bytes_in
    );

    let complete_ev = recv_matching(&mut rx, |e| {
        e.op.as_deref() == Some("CompleteMultipartUpload")
    })
    .await;
    assert_eq!(complete_ev.status, 200);
    assert_eq!(complete_ev.key.as_deref(), Some("big.bin"));
}
