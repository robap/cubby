//! End-to-end tests for webhook event notifications: the delivery engine
//! ([`Notifier`]) driving a real in-test HTTP receiver, plus the delivery
//! guarantees (never blocks, per-destination timeout, best-effort log-and-drop).
//!
//! The store/UI firing paths are covered from Step 7 onward; this file starts by
//! driving the notifier directly (as those paths will), against the shared
//! [`RecordingReceiver`].

mod common;

use std::time::{Duration, Instant};

use aws_sdk_s3::primitives::ByteStream;
use common::{RecordingReceiver, TestServer};
use cubby::db::{Db, NotificationDraft};
use cubby::events::{BusSignal, EventBus};
use cubby::notify::{EventKind, Notifier, ObjectEvent};
use serde_json::json;

fn base(server: &TestServer) -> String {
    format!("http://{}", server.addr)
}

/// Add a destination through the real seam (as `curl` would), returning its id.
async fn add_dest(base: &str, bucket: &str, body: serde_json::Value) -> i64 {
    let resp = reqwest::Client::new()
        .post(format!("{base}/_/api/buckets/{bucket}/notifications"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "seam should accept the destination");
    resp.json::<serde_json::Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap()
}

fn open_db() -> (tempfile::TempDir, Db) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("meta.sqlite")).unwrap();
    (tmp, db)
}

fn add_destination(db: &Db, bucket: &str, url: &str, timeout_ms: i64, format: &str) {
    db.create_bucket(bucket, 0).unwrap();
    db.insert_bucket_notification(&NotificationDraft {
        bucket: bucket.to_owned(),
        url: url.to_owned(),
        events: vec!["s3:ObjectCreated:*".to_owned()],
        prefix: None,
        suffix: None,
        format: format.to_owned(),
        timeout_ms,
        created_at: 0,
    })
    .unwrap();
}

/// Drain the bus until a synthetic `Webhook` event arrives, returning its note.
async fn next_webhook_note(events: &EventBus, timeout: Duration) -> Option<String> {
    let (_backlog, mut rx) = events.subscribe(None);
    // The caller fires *after* subscribing in these tests, so no backlog race.
    let start = Instant::now();
    while start.elapsed() < timeout {
        match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Ok(BusSignal::Event(ev))) if ev.op.as_deref() == Some("Webhook") => return ev.note,
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => return None,
            Err(_) => continue,
        }
    }
    None
}

// The engine delivers a matching event's payload to a live receiver.
#[tokio::test]
async fn notifier_delivers_matching_event_to_receiver() {
    let (_tmp, db) = open_db();
    let recv = RecordingReceiver::spawn().await;
    add_destination(&db, "uploads", &recv.url(), 5000, "s3-notification");

    let notifier = Notifier::new(db, EventBus::new(), true);
    notifier.notify(ObjectEvent::created(
        "uploads",
        "photos/cat.jpg",
        EventKind::Put,
        24173,
        "d41d8cd98f00b204e9800998ecf8427e",
    ));

    assert!(
        recv.wait_for(1, Duration::from_secs(2)).await,
        "receiver should get exactly one POST"
    );
    let req = &recv.requests()[0];
    assert_eq!(req.path, "/hook");
    let v = req.json();
    assert_eq!(v["Records"][0]["eventName"], "ObjectCreated:Put");
    assert_eq!(v["Records"][0]["s3"]["object"]["key"], "photos/cat.jpg");
    assert_eq!(v["Records"][0]["s3"]["object"]["size"], 24173);
}

// A non-matching event (wrong prefix) delivers nothing.
#[tokio::test]
async fn notifier_skips_non_matching_event() {
    let (_tmp, db) = open_db();
    let recv = RecordingReceiver::spawn().await;
    db.create_bucket("uploads", 0).unwrap();
    db.insert_bucket_notification(&NotificationDraft {
        bucket: "uploads".to_owned(),
        url: recv.url(),
        events: vec!["s3:ObjectCreated:*".to_owned()],
        prefix: Some("photos/".to_owned()),
        suffix: None,
        format: "s3-notification".to_owned(),
        timeout_ms: 5000,
        created_at: 0,
    })
    .unwrap();

    let notifier = Notifier::new(db, EventBus::new(), true);
    // Key outside the prefix.
    notifier.notify(ObjectEvent::created(
        "uploads",
        "docs/readme.md",
        EventKind::Put,
        1,
        "e",
    ));

    // Give the background task time; nothing should arrive.
    assert!(!recv.wait_for(1, Duration::from_millis(400)).await);
}

// A dead receiver never blocks the caller, and the failure is logged (a webhook
// line whose note marks the error).
#[tokio::test]
async fn dead_receiver_logs_failure_and_never_blocks() {
    let (_tmp, db) = open_db();
    // Port 1 refuses connections immediately.
    add_destination(
        &db,
        "uploads",
        "http://127.0.0.1:1/hook",
        5000,
        "s3-notification",
    );
    let events = EventBus::new();
    let notifier = Notifier::new(db, events.clone(), true);

    // notify() returns effectively instantly (it only spawns).
    let start = Instant::now();
    notifier.notify(ObjectEvent::created("uploads", "k", EventKind::Put, 1, "e"));
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "notify must return immediately"
    );

    let note = next_webhook_note(&events, Duration::from_secs(3)).await;
    let note = note.expect("a webhook delivery line should be logged");
    assert!(
        note.contains("error") && note.contains("127.0.0.1:1"),
        "note should mark the connect failure: {note:?}"
    );
}

// The per-destination timeout is honored: a short timeout against a slow
// receiver logs a timeout, while a generous timeout to the same receiver
// succeeds.
#[tokio::test]
async fn timeout_is_per_destination() {
    // Receiver sleeps ~600ms before replying 200.
    let recv = RecordingReceiver::spawn_with(600, 200).await;

    // Short timeout → timeout note.
    {
        let (_tmp, db) = open_db();
        add_destination(&db, "uploads", &recv.url(), 100, "s3-notification");
        let events = EventBus::new();
        let notifier = Notifier::new(db, events.clone(), true);
        notifier.notify(ObjectEvent::created("uploads", "k", EventKind::Put, 1, "e"));
        let note = next_webhook_note(&events, Duration::from_secs(3))
            .await
            .expect("timeout should be logged");
        assert!(note.contains("timeout"), "note: {note:?}");
    }

    // Generous timeout → 200 success note.
    {
        let (_tmp, db) = open_db();
        add_destination(&db, "uploads", &recv.url(), 5000, "s3-notification");
        let events = EventBus::new();
        let notifier = Notifier::new(db, events.clone(), true);
        notifier.notify(ObjectEvent::created("uploads", "k", EventKind::Put, 1, "e"));
        let note = next_webhook_note(&events, Duration::from_secs(3))
            .await
            .expect("success should be logged");
        assert!(note.contains("200"), "note: {note:?}");
    }
}

// --- Firing from the S3 store path (Step 7) ---------------------------------

// PutObject → exactly one ObjectCreated:Put with size/eTag equal to head-object.
#[tokio::test]
async fn s3_put_fires_object_created_put() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"]}),
    )
    .await;

    s3.put_object()
        .bucket("uploads")
        .key("photos/cat.jpg")
        .body(ByteStream::from_static(b"hello"))
        .send()
        .await
        .unwrap();

    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    let reqs = recv.requests();
    assert_eq!(reqs.len(), 1, "exactly one POST");
    let v = reqs[0].json();
    let rec = &v["Records"][0];
    assert_eq!(rec["eventName"], "ObjectCreated:Put");
    assert_eq!(rec["s3"]["bucket"]["name"], "uploads");
    assert_eq!(rec["s3"]["object"]["key"], "photos/cat.jpg");
    assert_eq!(rec["s3"]["object"]["size"], 5);

    // eTag equals head-object.
    let head = s3
        .head_object()
        .bucket("uploads")
        .key("photos/cat.jpg")
        .send()
        .await
        .unwrap();
    let head_etag = head.e_tag().unwrap().trim_matches('"').to_owned();
    assert_eq!(rec["s3"]["object"]["eTag"], head_etag);
}

// DeleteObject → one ObjectRemoved:Delete with no size/eTag.
#[tokio::test]
async fn s3_delete_fires_object_removed() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    // Removal-only subscription, so the prior PUT doesn't fire.
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectRemoved:*"]}),
    )
    .await;

    s3.put_object()
        .bucket("uploads")
        .key("k.txt")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();
    s3.delete_object()
        .bucket("uploads")
        .key("k.txt")
        .send()
        .await
        .unwrap();

    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    let reqs = recv.requests();
    assert_eq!(reqs.len(), 1, "only the delete fires");
    let obj = &reqs[0].json()["Records"][0];
    assert_eq!(obj["eventName"], "ObjectRemoved:Delete");
    assert!(obj["s3"]["object"].get("size").is_none());
    assert!(obj["s3"]["object"].get("eTag").is_none());
}

// CopyObject → ObjectCreated:Copy for the destination key.
#[tokio::test]
async fn s3_copy_fires_object_created_copy() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"]}),
    )
    .await;

    s3.put_object()
        .bucket("uploads")
        .key("a")
        .body(ByteStream::from_static(b"hello"))
        .send()
        .await
        .unwrap();
    s3.copy_object()
        .bucket("uploads")
        .key("b")
        .copy_source("uploads/a")
        .send()
        .await
        .unwrap();

    // Two creations fire (Put a, Copy b); find the Copy.
    assert!(recv.wait_for(2, Duration::from_secs(3)).await);
    let copy = recv
        .requests()
        .into_iter()
        .map(|r| r.json())
        .find(|v| v["Records"][0]["eventName"] == "ObjectCreated:Copy")
        .expect("a Copy event");
    assert_eq!(copy["Records"][0]["s3"]["object"]["key"], "b");
}

// DeleteObjects → one ObjectRemoved:Delete per key actually removed (ghost keys
// fire nothing).
#[tokio::test]
async fn s3_delete_objects_fires_one_per_removed_key() {
    use aws_sdk_s3::types::{Delete, ObjectIdentifier};

    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectRemoved:*"]}),
    )
    .await;

    for k in ["k1", "k2", "k3"] {
        s3.put_object()
            .bucket("uploads")
            .key(k)
            .body(ByteStream::from_static(b"x"))
            .send()
            .await
            .unwrap();
    }
    let delete = Delete::builder()
        .set_objects(Some(vec![
            ObjectIdentifier::builder().key("k1").build().unwrap(),
            ObjectIdentifier::builder().key("k2").build().unwrap(),
            ObjectIdentifier::builder().key("k3").build().unwrap(),
            ObjectIdentifier::builder().key("ghost").build().unwrap(),
        ]))
        .build()
        .unwrap();
    s3.delete_objects()
        .bucket("uploads")
        .delete(delete)
        .send()
        .await
        .unwrap();

    // Exactly three removals (not four — ghost never existed).
    assert!(recv.wait_for(3, Duration::from_secs(3)).await);
    // Give a beat for any (incorrect) 4th to arrive, then assert exactly 3.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let reqs = recv.requests();
    assert_eq!(reqs.len(), 3, "one per removed key, ghost excluded");
    for r in &reqs {
        assert_eq!(r.json()["Records"][0]["eventName"], "ObjectRemoved:Delete");
    }
}

// Multipart completion → one ObjectCreated:CompleteMultipartUpload whose eTag is
// the `-N` composite (not one event per part).
#[tokio::test]
async fn s3_multipart_complete_fires_once_with_composite_etag() {
    use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"]}),
    )
    .await;

    let create = s3
        .create_multipart_upload()
        .bucket("uploads")
        .key("big.bin")
        .send()
        .await
        .unwrap();
    let upload_id = create.upload_id().unwrap().to_owned();

    let mut completed = Vec::new();
    for part_number in 1..=2 {
        let out = s3
            .upload_part()
            .bucket("uploads")
            .key("big.bin")
            .upload_id(&upload_id)
            .part_number(part_number)
            .body(ByteStream::from_static(b"some-part-bytes"))
            .send()
            .await
            .unwrap();
        completed.push(
            CompletedPart::builder()
                .part_number(part_number)
                .e_tag(out.e_tag().unwrap())
                .build(),
        );
    }
    s3.complete_multipart_upload()
        .bucket("uploads")
        .key("big.bin")
        .upload_id(&upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(completed))
                .build(),
        )
        .send()
        .await
        .unwrap();

    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let reqs = recv.requests();
    assert_eq!(reqs.len(), 1, "exactly one completion event, not per-part");
    let rec = &reqs[0].json()["Records"][0];
    assert_eq!(rec["eventName"], "ObjectCreated:CompleteMultipartUpload");
    let etag = rec["s3"]["object"]["eTag"].as_str().unwrap();
    assert!(etag.ends_with("-2"), "composite etag: {etag}");
}

// --- Firing from the UI object path (Step 8) --------------------------------

// A UI upload and delete each fire a webhook (proving firing is at the store
// layer regardless of origin), while neither UI mutation appears in the live log
// as a PutObject/DeleteObject op — the intended divergence.
#[tokio::test]
async fn ui_mutations_fire_but_are_not_logged_as_s3_ops() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*", "s3:ObjectRemoved:*"]}),
    )
    .await;

    let (_backlog, mut rx) = server.events.subscribe(None);
    let http = reqwest::Client::new();
    let url = format!(
        "{}/_/api/buckets/uploads/objects/photos/x.jpg",
        base(&server)
    );

    // UI upload, then UI delete.
    let put = http
        .put(&url)
        .body(b"ui-bytes".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);
    let del = http.delete(&url).send().await.unwrap();
    assert_eq!(del.status(), 204);

    // Both webhooks fire.
    assert!(recv.wait_for(2, Duration::from_secs(3)).await);
    let names: Vec<String> = recv
        .requests()
        .into_iter()
        .map(|r| {
            r.json()["Records"][0]["eventName"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(names.contains(&"ObjectCreated:Put".to_owned()), "{names:?}");
    assert!(
        names.contains(&"ObjectRemoved:Delete".to_owned()),
        "{names:?}"
    );

    // Drain the live log: the UI mutations are NOT logged as S3 ops (only the
    // synthetic Webhook delivery lines appear).
    let mut ops = Vec::new();
    while let Ok(Ok(signal)) = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await {
        if let BusSignal::Event(ev) = signal {
            ops.push(ev.op.clone());
        }
    }
    assert!(
        !ops.iter().any(|op| op.as_deref() == Some("PutObject")),
        "UI upload must not log a PutObject op: {ops:?}"
    );
    assert!(
        !ops.iter().any(|op| op.as_deref() == Some("DeleteObject")),
        "UI delete must not log a DeleteObject op: {ops:?}"
    );
    // The webhook delivery lines DO appear (delivery is visible in the log).
    assert!(
        ops.iter().any(|op| op.as_deref() == Some("Webhook")),
        "webhook delivery lines should appear: {ops:?}"
    );
}

// --- Format selector (end-to-end) -------------------------------------------

// The same PUT renders as `{"Records":[…]}` for an s3-notification destination
// and as the EventBridge envelope for an eventbridge destination.
#[tokio::test]
async fn format_selector_renders_both_shapes_for_one_put() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let s3n = RecordingReceiver::spawn().await;
    let eb = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": s3n.url(), "events": ["s3:ObjectCreated:*"], "format": "s3-notification"}),
    )
    .await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": eb.url(), "events": ["s3:ObjectCreated:*"], "format": "eventbridge"}),
    )
    .await;

    s3_put(&s3, "uploads", "photos/cat.jpg").await;

    assert!(s3n.wait_for(1, Duration::from_secs(3)).await);
    assert!(eb.wait_for(1, Duration::from_secs(3)).await);

    // s3-notification shape.
    let a = s3n.requests()[0].json();
    assert_eq!(a["Records"][0]["eventName"], "ObjectCreated:Put");
    assert!(a.get("source").is_none());

    // eventbridge shape: source/detail-type/detail.object.key, lowercase etag.
    let b = eb.requests()[0].json();
    assert_eq!(b["source"], "aws.s3");
    assert_eq!(b["detail-type"], "Object Created");
    assert_eq!(b["detail"]["object"]["key"], "photos/cat.jpg");
    assert!(b["detail"]["object"].get("etag").is_some());
    assert!(b.get("Records").is_none());
}

// A destination with no `format` defaults to the s3-notification shape.
#[tokio::test]
async fn default_format_is_s3_notification() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    // No `format` in the body.
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"]}),
    )
    .await;

    s3_put(&s3, "uploads", "k").await;
    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    assert!(recv.requests()[0].json().get("Records").is_some());
}

// A bucket with no destinations never POSTs anywhere.
#[tokio::test]
async fn no_config_no_delivery() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    // Receiver exists but is never configured as a destination.

    s3_put(&s3, "uploads", "k").await;
    // Nothing should arrive.
    assert!(!recv.wait_for(1, Duration::from_millis(500)).await);
}

// --- Filtering & fan-out end-to-end (Step 9) --------------------------------

async fn s3_put(s3: &aws_sdk_s3::Client, bucket: &str, key: &str) {
    s3.put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();
}

/// The single received key (asserts exactly one POST arrived first).
fn only_key(recv: &RecordingReceiver) -> String {
    let reqs = recv.requests();
    assert_eq!(reqs.len(), 1, "expected exactly one delivery");
    reqs[0].json()["Records"][0]["s3"]["object"]["key"]
        .as_str()
        .unwrap()
        .to_owned()
}

// Prefix filter: photos/ fires, docs/ doesn't — one POST across both PUTs.
#[tokio::test]
async fn prefix_filter_gates_delivery() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"], "prefix": "photos/"}),
    )
    .await;

    s3_put(&s3, "uploads", "photos/cat.jpg").await;
    s3_put(&s3, "uploads", "docs/readme.md").await;

    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(only_key(&recv), "photos/cat.jpg");
}

// Suffix filter: .jpg fires, .png doesn't.
#[tokio::test]
async fn suffix_filter_gates_delivery() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"], "suffix": ".jpg"}),
    )
    .await;

    s3_put(&s3, "uploads", "a.jpg").await;
    s3_put(&s3, "uploads", "a.png").await;

    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(only_key(&recv), "a.jpg");
}

// Event filter: a created-only subscription fires on PUT but not the later
// DELETE of the same key.
#[tokio::test]
async fn event_filter_gates_delivery() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let recv = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": recv.url(), "events": ["s3:ObjectCreated:*"]}),
    )
    .await;

    s3_put(&s3, "uploads", "k").await;
    s3.delete_object()
        .bucket("uploads")
        .key("k")
        .send()
        .await
        .unwrap();

    assert!(recv.wait_for(1, Duration::from_secs(3)).await);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let reqs = recv.requests();
    assert_eq!(reqs.len(), 1, "only the PUT fires");
    assert_eq!(
        reqs[0].json()["Records"][0]["eventName"],
        "ObjectCreated:Put"
    );
}

// Per-path routing: photos/→A, invoices/→B, each isolated to its own URL.
#[tokio::test]
async fn per_path_routing_to_distinct_urls() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let a = RecordingReceiver::spawn().await;
    let b = RecordingReceiver::spawn().await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": a.url(), "events": ["s3:ObjectCreated:*"], "prefix": "photos/"}),
    )
    .await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": b.url(), "events": ["s3:ObjectCreated:*"], "prefix": "invoices/"}),
    )
    .await;

    s3_put(&s3, "uploads", "photos/cat.jpg").await;
    s3_put(&s3, "uploads", "invoices/2026.pdf").await;

    assert!(a.wait_for(1, Duration::from_secs(3)).await);
    assert!(b.wait_for(1, Duration::from_secs(3)).await);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(only_key(&a), "photos/cat.jpg");
    assert_eq!(only_key(&b), "invoices/2026.pdf");
}

// Overlapping filters fan out: two destinations both matching photos/cat.jpg
// each receive the PUT (cubby allows overlap, diverging from AWS's rejection).
#[tokio::test]
async fn overlapping_filters_fan_out() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("uploads").send().await.unwrap();
    let a = RecordingReceiver::spawn().await;
    let b = RecordingReceiver::spawn().await;
    // Both match photos/cat.jpg: one by prefix, one by suffix.
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": a.url(), "events": ["s3:ObjectCreated:*"], "prefix": "photos/"}),
    )
    .await;
    add_dest(
        &base(&server),
        "uploads",
        json!({"url": b.url(), "events": ["s3:ObjectCreated:*"], "suffix": ".jpg"}),
    )
    .await;

    s3_put(&s3, "uploads", "photos/cat.jpg").await;

    assert!(a.wait_for(1, Duration::from_secs(3)).await);
    assert!(b.wait_for(1, Duration::from_secs(3)).await);
    assert_eq!(only_key(&a), "photos/cat.jpg");
    assert_eq!(only_key(&b), "photos/cat.jpg");
}
