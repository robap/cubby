//! End-to-end tests for the bucket-browser JSON seam (`/_/api/…`): bucket list,
//! folder-view listing, search, byte streaming, and UI upload/delete — driven
//! with `reqwest` (as `curl` would) and cross-checked against the `aws-sdk-s3`
//! client and the filesystem.

mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::TestServer;

fn base(server: &TestServer) -> String {
    format!("http://{}", server.addr)
}

async fn put(client: &aws_sdk_s3::Client, bucket: &str, key: &str, body: &'static [u8]) {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from_static(body))
        .send()
        .await
        .unwrap();
}

// Box B1 — `GET /_/api/buckets` mirrors S3, with stats.
#[tokio::test]
async fn buckets_lists_with_counts_and_size() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    put(&s3, "demo", "a.txt", b"hello").await;

    let http = reqwest::Client::new();
    let body: serde_json::Value = http
        .get(format!("{}/_/api/buckets", base(&server)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let b = &body["buckets"][0];
    assert_eq!(b["name"], "demo");
    assert_eq!(b["object_count"], 1);
    assert_eq!(b["size"], 5);
    assert!(b["created_at"].as_str().unwrap().contains('T'), "ISO date");
}

// Box B2 — folder view via delimiter=/.
#[tokio::test]
async fn objects_folder_view_rolls_up_prefixes() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    put(&s3, "demo", "a/1.txt", b"x").await;
    put(&s3, "demo", "a/2.txt", b"x").await;
    put(&s3, "demo", "b.txt", b"x").await;

    let http = reqwest::Client::new();
    let body: serde_json::Value = http
        .get(format!(
            "{}/_/api/buckets/demo/objects?delimiter=/",
            base(&server)
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["common_prefixes"][0], "a/");
    let objs: Vec<&str> = body["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["key"].as_str().unwrap())
        .collect();
    assert_eq!(objs, ["b.txt"]);
}

// Box B3 — scoped, global, and substring search.
#[tokio::test]
async fn search_is_flat_scoped_and_substring() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    s3.create_bucket().bucket("other").send().await.unwrap();
    put(&s3, "demo", "a/report.pdf", b"x").await;
    put(&s3, "demo", "logs/report-2.txt", b"x").await;
    put(&s3, "demo", "photos/cat.jpg", b"x").await;
    put(&s3, "other", "report.csv", b"x").await;

    let http = reqwest::Client::new();

    // Scoped: exactly the two demo report keys, flat (no rollup).
    let scoped: serde_json::Value = http
        .get(format!(
            "{}/_/api/search?q=report&bucket=demo",
            base(&server)
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let keys: Vec<&str> = scoped["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["a/report.pdf", "logs/report-2.txt"]);

    // Global: spans buckets, each tagged with its bucket.
    let global: serde_json::Value = http
        .get(format!("{}/_/api/search?q=report", base(&server)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let buckets: Vec<&str> = global["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["bucket"].as_str().unwrap())
        .collect();
    assert!(buckets.contains(&"demo") && buckets.contains(&"other"));

    // Substring, not prefix: `port` matches mid-key.
    let sub: serde_json::Value = http
        .get(format!("{}/_/api/search?q=port&bucket=demo", base(&server)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let keys: Vec<&str> = sub["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"a/report.pdf"));
}

// Box B4 — content streaming with Range.
#[tokio::test]
async fn content_streams_bytes_and_honors_range() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    put(&s3, "demo", "hello.txt", b"hello world").await;

    let http = reqwest::Client::new();
    let url = format!(
        "{}/_/api/buckets/demo/objects/hello.txt?content",
        base(&server)
    );

    let full = http.get(&url).send().await.unwrap();
    assert_eq!(full.status(), 200);
    assert_eq!(full.bytes().await.unwrap().as_ref(), b"hello world");

    let ranged = http
        .get(&url)
        .header("Range", "bytes=0-4")
        .send()
        .await
        .unwrap();
    assert_eq!(ranged.status(), 206);
    assert_eq!(
        ranged.headers().get("content-range").unwrap(),
        "bytes 0-4/11"
    );
    assert_eq!(ranged.bytes().await.unwrap().as_ref(), b"hello");
}

// Box B5 — UI upload lands a real file, is visible to S3, and is NOT logged.
#[tokio::test]
async fn ui_upload_and_delete_are_real_and_unlogged() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    let (_backlog, mut rx) = server.events.subscribe(None);

    let http = reqwest::Client::new();
    let url = format!("{}/_/api/buckets/demo/objects/x.bin", base(&server));

    // Upload via the UI seam.
    let resp = http
        .put(&url)
        .body(b"ui-bytes".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let out: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(out["size"], 8);

    // The bytes are a real file on disk.
    let path = server.object_path("demo", "x.bin");
    assert_eq!(std::fs::read(&path).unwrap(), b"ui-bytes");

    // And the authed S3 side sees the UI-written object.
    let got = s3
        .get_object()
        .bucket("demo")
        .key("x.bin")
        .send()
        .await
        .unwrap();
    let bytes = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(bytes.as_ref(), b"ui-bytes");

    // Delete via the UI seam removes the file.
    let del = http.delete(&url).send().await.unwrap();
    assert_eq!(del.status(), 204);
    assert!(!path.exists(), "deleted file should be gone");

    // Neither the UI upload nor delete appears in the live log — only the SDK's
    // GetObject above (decision #2). Drain the ring and assert no UI mutation.
    let mut ops = Vec::new();
    while let Ok(Ok(signal)) =
        tokio::time::timeout(std::time::Duration::from_millis(150), rx.recv()).await
    {
        if let cubby::events::BusSignal::Event(ev) = signal {
            ops.push((ev.op.clone(), ev.key.clone()));
        }
    }
    assert!(
        !ops.iter().any(|(op, _)| op.as_deref() == Some("PutObject")),
        "UI upload must not be logged: {ops:?}"
    );
    assert!(
        !ops.iter()
            .any(|(op, _)| op.as_deref() == Some("DeleteObject")),
        "UI delete must not be logged: {ops:?}"
    );
}

// Group C meta — object metadata matches head-object.
#[tokio::test]
async fn object_meta_matches_head_object() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    put(&s3, "demo", "img.png", b"\x89PNG....").await;

    let head = s3
        .head_object()
        .bucket("demo")
        .key("img.png")
        .send()
        .await
        .unwrap();
    let head_etag = head.e_tag().unwrap().trim_matches('"').to_owned();

    let http = reqwest::Client::new();
    let meta: serde_json::Value = http
        .get(format!(
            "{}/_/api/buckets/demo/objects/img.png",
            base(&server)
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(meta["etag"], head_etag);
    assert_eq!(meta["size"], 8);
    assert_eq!(meta["storage_class"], "STANDARD");

    // Missing key → 404 NoSuchKey.
    let missing = http
        .get(format!("{}/_/api/buckets/demo/objects/nope", base(&server)))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    let err: serde_json::Value = missing.json().await.unwrap();
    assert_eq!(err["error"]["code"], "NoSuchKey");
}

// Group C presign (C2) — a minted GET URL resolves credential-less, and a
// lapsed 1s-expiry URL is rejected.
#[tokio::test]
async fn presigned_get_url_resolves_then_expires() {
    let server = TestServer::spawn().await;
    let s3 = server.client();
    s3.create_bucket().bucket("demo").send().await.unwrap();
    put(&s3, "demo", "secret.txt", b"top-secret").await;

    // A plain HTTP client — it presents NO SigV4 of its own.
    let http = reqwest::Client::new();

    let minted: serde_json::Value = http
        .post(format!("{}/_/api/presign", base(&server)))
        .json(&serde_json::json!({
            "method": "GET",
            "bucket": "demo",
            "key": "secret.txt",
            "expires_in_s": 3600,
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let url = minted["url"].as_str().expect("url in response");
    assert!(minted["expires_at"].as_str().unwrap().contains('T'));

    // Fetching the presigned URL with no credentials returns the bytes.
    let got = http.get(url).send().await.unwrap();
    assert_eq!(got.status(), 200, "presigned GET should resolve");
    assert_eq!(got.bytes().await.unwrap().as_ref(), b"top-secret");

    // A 1-second URL, used after it lapses, is rejected.
    let short: serde_json::Value = http
        .post(format!("{}/_/api/presign", base(&server)))
        .json(&serde_json::json!({
            "method": "GET",
            "bucket": "demo",
            "key": "secret.txt",
            "expires_in_s": 1,
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let short_url = short["url"].as_str().unwrap().to_owned();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let expired = http.get(&short_url).send().await.unwrap();
    assert_eq!(expired.status(), 403, "expired presigned URL should 403");
}

// Box B7 — POST /_/api/buckets creates a bucket for the UI (dir-before-row),
// and both the seam and the S3 API see it afterward.
#[tokio::test]
async fn post_bucket_creates_and_s3_sees_it() {
    let server = TestServer::spawn().await;
    let http = reqwest::Client::new();

    let resp = http
        .post(format!("{}/_/api/buckets", base(&server)))
        .json(&serde_json::json!({ "name": "demo" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "demo");

    // Directory is on disk (dir-before-row ordering, like the S3 path).
    assert!(server.datadir.bucket_dir("demo").is_dir());

    // The seam lists it.
    let listed: serde_json::Value = http
        .get(format!("{}/_/api/buckets", base(&server)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["buckets"][0]["name"], "demo");

    // The S3 API sees it too.
    let s3 = server.client();
    let out = s3.list_buckets().send().await.unwrap();
    assert!(out.buckets().iter().any(|b| b.name() == Some("demo")));
}

// Box B7 — POST rejects an invalid bucket name with a 400 envelope.
#[tokio::test]
async fn post_bucket_rejects_invalid_name() {
    let server = TestServer::spawn().await;
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{}/_/api/buckets", base(&server)))
        .json(&serde_json::json!({ "name": "_bad" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "InvalidBucketName");
    // Nothing was created on disk.
    assert!(!server.datadir.bucket_dir("_bad").exists());
}

// Box B7 — POST is a 409 conflict when the bucket already exists.
#[tokio::test]
async fn post_bucket_conflicts_on_duplicate() {
    let server = TestServer::spawn().await;
    server
        .client()
        .create_bucket()
        .bucket("demo")
        .send()
        .await
        .unwrap();
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{}/_/api/buckets", base(&server)))
        .json(&serde_json::json!({ "name": "demo" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "BucketAlreadyExists");
}

// --- Notification config seam (Step 5) --------------------------------------

// POST a valid destination → 201 + id; GET lists it; DELETE removes it.
#[tokio::test]
async fn notification_config_crud_round_trips_through_the_seam() {
    let server = TestServer::spawn().await;
    server
        .client()
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    let http = reqwest::Client::new();
    let base_url = format!("{}/_/api/buckets/uploads/notifications", base(&server));

    // Empty to start.
    let listed: serde_json::Value = http
        .get(&base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["notifications"].as_array().unwrap().len(), 0);

    // POST a valid destination → 201 with an id and the echoed fields.
    let created = http
        .post(&base_url)
        .json(&serde_json::json!({
            "url": "http://localhost:3000/hook",
            "events": ["s3:ObjectCreated:*"],
            "prefix": "photos/",
            "suffix": ".jpg",
            "format": "s3-notification",
            "timeout_ms": 4000
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let created: serde_json::Value = created.json().await.unwrap();
    let id = created["id"].as_i64().expect("id present");
    assert!(id >= 1);
    assert_eq!(created["url"], "http://localhost:3000/hook");
    assert_eq!(created["prefix"], "photos/");
    assert_eq!(created["timeout_ms"], 4000);

    // GET now lists exactly that destination.
    let listed: serde_json::Value = http
        .get(&base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let arr = listed["notifications"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
    assert_eq!(arr[0]["events"][0], "s3:ObjectCreated:*");

    // DELETE removes it → 204, and the list is empty again.
    let del = http
        .delete(format!("{base_url}/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);
    let listed: serde_json::Value = http
        .get(&base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["notifications"].as_array().unwrap().len(), 0);

    // A second delete of the same id is a 404.
    let del2 = http
        .delete(format!("{base_url}/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(del2.status(), 404);
}

// Default format is s3-notification; timeout defaults to 5000; empty
// prefix/suffix become null (no constraint).
#[tokio::test]
async fn notification_create_applies_defaults() {
    let server = TestServer::spawn().await;
    server
        .client()
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    let http = reqwest::Client::new();
    let base_url = format!("{}/_/api/buckets/uploads/notifications", base(&server));

    let created: serde_json::Value = http
        .post(&base_url)
        .json(&serde_json::json!({
            "url": "http://localhost:3000/hook",
            "events": ["s3:ObjectCreated:Put"]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["format"], "s3-notification");
    assert_eq!(created["timeout_ms"], 5000);
    assert!(created["prefix"].is_null());
    assert!(created["suffix"].is_null());
}

// Invalid destinations are rejected at write time (400) and persist nothing.
#[tokio::test]
async fn notification_create_rejects_invalid_destinations() {
    let server = TestServer::spawn().await;
    server
        .client()
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    let http = reqwest::Client::new();
    let base_url = format!("{}/_/api/buckets/uploads/notifications", base(&server));

    let bad_bodies = [
        // https:// url (out of scope for v0.2).
        serde_json::json!({"url":"https://x/hook","events":["s3:ObjectCreated:*"]}),
        // Unknown event token.
        serde_json::json!({"url":"http://x/hook","events":["s3:ObjectMutated:*"]}),
        // Invalid format.
        serde_json::json!({"url":"http://x/hook","events":["s3:ObjectCreated:*"],"format":"protobuf"}),
        // Non-positive timeout.
        serde_json::json!({"url":"http://x/hook","events":["s3:ObjectCreated:*"],"timeout_ms":0}),
    ];
    for body in bad_bodies {
        let resp = http.post(&base_url).json(&body).send().await.unwrap();
        assert_eq!(resp.status(), 400, "should reject {body}");
    }

    // Nothing was persisted.
    let listed: serde_json::Value = http
        .get(&base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["notifications"].as_array().unwrap().len(), 0);
}

// POST to a nonexistent bucket is a 404 (config is bucket state).
#[tokio::test]
async fn notification_create_on_missing_bucket_is_404() {
    let server = TestServer::spawn().await;
    let http = reqwest::Client::new();
    let resp = http
        .post(format!(
            "{}/_/api/buckets/ghost/notifications",
            base(&server)
        ))
        .json(&serde_json::json!({
            "url": "http://localhost:3000/hook",
            "events": ["s3:ObjectCreated:*"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "NoSuchBucket");
}
