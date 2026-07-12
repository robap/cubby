//! End-to-end tests driving the real `aws-sdk-s3` client against an in-process
//! cubby server — the inner-loop proxy for the AWS CLI acceptance criteria.

mod common;

use aws_sdk_s3::error::ProvideErrorMetadata;
use common::TestServer;

#[tokio::test]
async fn list_buckets_empty_on_fresh_server() {
    let server = TestServer::spawn().await;
    let out = server
        .client()
        .list_buckets()
        .send()
        .await
        .expect("list-buckets should succeed");
    assert!(out.buckets().is_empty(), "fresh server has no buckets");
}

#[tokio::test]
async fn wrong_secret_is_signature_does_not_match() {
    let server = TestServer::spawn().await;
    let err = server
        .client_with(common::ACCESS_KEY, "the-wrong-secret")
        .list_buckets()
        .send()
        .await
        .expect_err("a bad secret must be rejected");
    assert_eq!(
        err.code(),
        Some("SignatureDoesNotMatch"),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn create_bucket_makes_dir_and_row_and_lists() {
    let server = TestServer::spawn().await;
    let client = server.client();

    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .expect("create-bucket 200");
    assert!(
        server.datadir.bucket_dir("uploads").is_dir(),
        "bucket dir created on disk"
    );

    let out = client.list_buckets().send().await.unwrap();
    let names: Vec<&str> = out.buckets().iter().filter_map(|b| b.name()).collect();
    assert_eq!(names, ["uploads"]);
}

#[tokio::test]
async fn recreate_bucket_is_already_owned_by_you() {
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();

    let err = client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .expect_err("re-create must fail");
    assert_eq!(
        err.code(),
        Some("BucketAlreadyOwnedByYou"),
        "unexpected: {err:?}"
    );
}

#[tokio::test]
async fn create_bucket_ignores_region() {
    use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration};
    let server = TestServer::spawn().await;
    let cfg = CreateBucketConfiguration::builder()
        .location_constraint(BucketLocationConstraint::from("eu-west-1"))
        .build();
    server
        .client()
        .create_bucket()
        .bucket("regional")
        .create_bucket_configuration(cfg)
        .send()
        .await
        .expect("any region is accepted and ignored");
    assert!(server.datadir.bucket_dir("regional").is_dir());
}

#[tokio::test]
async fn head_bucket_present_and_missing() {
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();

    client
        .head_bucket()
        .bucket("uploads")
        .send()
        .await
        .expect("head existing bucket → 200");

    // HeadBucket has no response body, so S3 cannot carry the `NoSuchBucket`
    // code — a missing bucket is a bodyless 404, which the SDK models as
    // `NotFound`. The observable contract is the 404.
    let err = client
        .head_bucket()
        .bucket("ghost")
        .send()
        .await
        .expect_err("missing bucket");
    assert!(
        err.into_service_error().is_not_found(),
        "missing bucket → 404 NotFound"
    );
}

#[tokio::test]
async fn delete_empty_bucket_removes_dir() {
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    assert!(server.datadir.bucket_dir("uploads").is_dir());

    client
        .delete_bucket()
        .bucket("uploads")
        .send()
        .await
        .expect("delete empty bucket → 200");
    assert!(
        !server.datadir.bucket_dir("uploads").exists(),
        "bucket dir removed"
    );

    // And it no longer heads (bodyless 404 → NotFound, as above).
    let err = client
        .head_bucket()
        .bucket("uploads")
        .send()
        .await
        .expect_err("gone");
    assert!(err.into_service_error().is_not_found());
}

#[tokio::test]
async fn delete_missing_bucket_is_no_such_bucket() {
    let server = TestServer::spawn().await;
    let err = server
        .client()
        .delete_bucket()
        .bucket("ghost")
        .send()
        .await
        .expect_err("missing");
    assert_eq!(err.code(), Some("NoSuchBucket"), "unexpected: {err:?}");
}

#[tokio::test]
async fn delete_non_empty_bucket_is_bucket_not_empty() {
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    server.seed_object_row("uploads", "keep.txt");

    let err = client
        .delete_bucket()
        .bucket("uploads")
        .send()
        .await
        .expect_err("non-empty");
    assert_eq!(err.code(), Some("BucketNotEmpty"), "unexpected: {err:?}");
    // Bucket survives the rejected delete.
    assert!(server.datadir.bucket_dir("uploads").is_dir());
}

fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    hex::encode(Md5::digest(data))
}

async fn make_bucket(client: &aws_sdk_s3::Client, bucket: &str) {
    client.create_bucket().bucket(bucket).send().await.unwrap();
}

#[tokio::test]
async fn put_object_writes_real_bytes_and_correct_etag() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;

    let data = b"hello cubby, these are real cat-able bytes on disk".to_vec();
    let out = client
        .put_object()
        .bucket("uploads")
        .key("report.pdf")
        .body(ByteStream::from(data.clone()))
        .send()
        .await
        .expect("put-object 200");

    // Bytes are a real file on disk, byte-for-byte.
    let on_disk = std::fs::read(server.object_path("uploads", "report.pdf")).expect("file exists");
    assert_eq!(on_disk, data, "on-disk bytes match the body (cmp-clean)");

    // ETag is the quoted hex MD5 of the body.
    assert_eq!(
        out.e_tag(),
        Some(format!("\"{}\"", md5_hex(&data)).as_str())
    );

    // The atomic write left nothing behind in .tmp/.
    let leftovers: Vec<_> = std::fs::read_dir(server.datadir.tmp_dir())
        .unwrap()
        .collect();
    assert!(leftovers.is_empty(), "no temp files left after rename");
}

#[tokio::test]
async fn put_object_creates_nested_dirs() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;

    let data = b"meow".to_vec();
    client
        .put_object()
        .bucket("uploads")
        .key("photos/cat.jpg")
        .body(ByteStream::from(data.clone()))
        .send()
        .await
        .unwrap();

    let path = server.object_path("uploads", "photos/cat.jpg");
    assert!(path.is_file(), "nested dirs created: {}", path.display());
    assert_eq!(std::fs::read(path).unwrap(), data);
}

#[tokio::test]
async fn put_object_illegal_key_stored_percent_encoded() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;

    client
        .put_object()
        .bucket("uploads")
        .key("weird:name?.txt")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .expect("illegal-char key round-trips via the SDK");

    let path = server.object_path("uploads", "weird:name?.txt");
    assert!(path.is_file(), "stored at derived path: {}", path.display());
    let fname = path.file_name().unwrap().to_string_lossy();
    assert!(
        !fname.contains(':') && !fname.contains('?'),
        "no raw illegal chars: {fname}"
    );
}

#[tokio::test]
async fn put_object_to_missing_bucket_is_no_such_bucket() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let err = server
        .client()
        .put_object()
        .bucket("ghost")
        .key("k")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .expect_err("put to missing bucket");
    assert_eq!(err.code(), Some("NoSuchBucket"), "unexpected: {err:?}");
}

#[tokio::test]
async fn head_object_returns_metadata_fields() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;

    let data = b"a report of exactly some length".to_vec();
    client
        .put_object()
        .bucket("uploads")
        .key("report.pdf")
        .content_type("application/pdf")
        .body(ByteStream::from(data.clone()))
        .send()
        .await
        .unwrap();

    let head = client
        .head_object()
        .bucket("uploads")
        .key("report.pdf")
        .send()
        .await
        .expect("head 200");
    assert_eq!(head.content_length(), Some(data.len() as i64));
    assert_eq!(
        head.e_tag(),
        Some(format!("\"{}\"", md5_hex(&data)).as_str())
    );
    assert_eq!(head.content_type(), Some("application/pdf"));
    assert!(head.last_modified().is_some(), "LastModified present");
}

#[tokio::test]
async fn head_object_default_content_type_is_octet_stream() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;
    client
        .put_object()
        .bucket("uploads")
        .key("blob")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();

    let head = client
        .head_object()
        .bucket("uploads")
        .key("blob")
        .send()
        .await
        .unwrap();
    assert_eq!(head.content_type(), Some("application/octet-stream"));
}

#[tokio::test]
async fn head_object_returns_user_metadata() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;
    client
        .put_object()
        .bucket("uploads")
        .key("m")
        .metadata("team", "infra")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();

    let head = client
        .head_object()
        .bucket("uploads")
        .key("m")
        .send()
        .await
        .unwrap();
    assert_eq!(
        head.metadata()
            .and_then(|m| m.get("team"))
            .map(String::as_str),
        Some("infra")
    );
}

#[tokio::test]
async fn head_missing_object_is_404() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;
    // HeadObject has no body, so (like HeadBucket) a missing key is a bodyless
    // 404 the SDK models as NotFound. GET (with a body) returns NoSuchKey.
    let err = client
        .head_object()
        .bucket("uploads")
        .key("ghost")
        .send()
        .await
        .expect_err("missing");
    assert!(err.into_service_error().is_not_found(), "missing key → 404");
}

#[tokio::test]
async fn get_object_full_is_cmp_clean_with_headers() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;

    let data: Vec<u8> = (0..500u32).map(|i| (i % 251) as u8).collect();
    client
        .put_object()
        .bucket("uploads")
        .key("report.pdf")
        .content_type("application/pdf")
        .body(ByteStream::from(data.clone()))
        .send()
        .await
        .unwrap();

    let out = client
        .get_object()
        .bucket("uploads")
        .key("report.pdf")
        .send()
        .await
        .expect("get 200");
    assert_eq!(out.content_length(), Some(data.len() as i64));
    assert_eq!(out.content_type(), Some("application/pdf"));
    assert_eq!(
        out.e_tag(),
        Some(format!("\"{}\"", md5_hex(&data)).as_str())
    );

    let body = out.body.collect().await.unwrap().into_bytes();
    assert_eq!(
        body.as_ref(),
        data.as_slice(),
        "streamed bytes match source (cmp-clean)"
    );
}

#[tokio::test]
async fn get_object_range_is_206_partial_slice() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;

    let data: Vec<u8> = (0..200u32).map(|i| i as u8).collect();
    client
        .put_object()
        .bucket("uploads")
        .key("blob")
        .body(ByteStream::from(data.clone()))
        .send()
        .await
        .unwrap();

    let out = client
        .get_object()
        .bucket("uploads")
        .key("blob")
        .range("bytes=0-99")
        .send()
        .await
        .expect("ranged get");
    assert_eq!(out.content_range(), Some("bytes 0-99/200"));
    assert_eq!(out.content_length(), Some(100));
    let body = out.body.collect().await.unwrap().into_bytes();
    assert_eq!(body.len(), 100, "exactly the requested 100 bytes");
    assert_eq!(
        body.as_ref(),
        &data[0..100],
        "first 100 bytes of the source"
    );
}

#[tokio::test]
async fn get_missing_object_is_no_such_key() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;
    // GET carries a body, so the NoSuchKey code is transmitted (unlike HEAD).
    let err = client
        .get_object()
        .bucket("uploads")
        .key("ghost")
        .send()
        .await
        .expect_err("missing");
    assert_eq!(err.code(), Some("NoSuchKey"), "unexpected: {err:?}");
}

#[tokio::test]
async fn delete_object_removes_row_and_file() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;
    client
        .put_object()
        .bucket("uploads")
        .key("report.pdf")
        .body(ByteStream::from_static(b"bytes"))
        .send()
        .await
        .unwrap();
    let path = server.object_path("uploads", "report.pdf");
    assert!(path.is_file());

    client
        .delete_object()
        .bucket("uploads")
        .key("report.pdf")
        .send()
        .await
        .expect("delete 200");
    assert!(!path.exists(), "file unlinked after row delete");

    // Subsequent GET → NoSuchKey; HEAD → 404.
    let get_err = client
        .get_object()
        .bucket("uploads")
        .key("report.pdf")
        .send()
        .await
        .expect_err("gone");
    assert_eq!(get_err.code(), Some("NoSuchKey"));
    let head_err = client
        .head_object()
        .bucket("uploads")
        .key("report.pdf")
        .send()
        .await
        .expect_err("gone");
    assert!(head_err.into_service_error().is_not_found());
}

#[tokio::test]
async fn delete_missing_object_is_idempotent_success() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "uploads").await;
    // Deleting a key that was never there still succeeds.
    client
        .delete_object()
        .bucket("uploads")
        .key("never-existed")
        .send()
        .await
        .expect("idempotent 200");
}

#[tokio::test]
async fn get_and_head_on_missing_bucket_are_no_such_bucket() {
    let server = TestServer::spawn().await;
    let client = server.client();
    // No bucket created at all.
    let get_err = client
        .get_object()
        .bucket("ghost")
        .key("k")
        .send()
        .await
        .expect_err("no bucket");
    assert_eq!(
        get_err.code(),
        Some("NoSuchBucket"),
        "GET on missing bucket: {get_err:?}"
    );

    // HEAD is bodyless → 404 NotFound regardless of specific code.
    let head_err = client
        .head_object()
        .bucket("ghost")
        .key("k")
        .send()
        .await
        .expect_err("no bucket");
    assert!(
        head_err.into_service_error().is_not_found(),
        "HEAD on missing bucket → 404"
    );
}

#[tokio::test]
async fn underscore_prefix_serves_the_web_ui() {
    let server = TestServer::spawn().await;
    // A plain HTTP GET (no SigV4) — the routing layer intercepts before auth
    // and serves the embedded zero UI (Phase 5 replaced the 501 placeholder).
    let (status, body) = raw_http_get(server.addr, "/_/").await;
    assert_eq!(status, 200, "/_/ must serve the embedded UI");
    assert!(body.contains(r#"id="app""#), "UI index body: {body}");
    assert!(
        !body.contains("Phase 5"),
        "the placeholder body must be gone"
    );
}

/// The five-key `photos` fixture from the spec, seeded as real objects.
async fn seed_photos(client: &aws_sdk_s3::Client) {
    use aws_sdk_s3::primitives::ByteStream;
    make_bucket(client, "photos").await;
    for key in [
        "notes.txt",
        "photos/index.md",
        "photos/2024/a.jpg",
        "photos/2024/b.jpg",
        "photos/2025/c.jpg",
    ] {
        client
            .put_object()
            .bucket("photos")
            .key(key)
            .body(ByteStream::from_static(b"x"))
            .send()
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn v2_prefix_delimiter_groups_and_counts() {
    let server = TestServer::spawn().await;
    let client = server.client();
    seed_photos(&client).await;

    let out = client
        .list_objects_v2()
        .bucket("photos")
        .prefix("photos/")
        .delimiter("/")
        .send()
        .await
        .expect("list v2");

    let cps: Vec<&str> = out
        .common_prefixes()
        .iter()
        .filter_map(|c| c.prefix())
        .collect();
    assert_eq!(cps, ["photos/2024/", "photos/2025/"]);
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys, ["photos/index.md"]);
    assert_eq!(out.key_count(), Some(3), "keys + prefixes counted together");
    assert_eq!(out.is_truncated(), Some(false));
    // Echoed request fields.
    assert_eq!(out.name(), Some("photos"));
    assert_eq!(out.prefix(), Some("photos/"));
    assert_eq!(out.delimiter(), Some("/"));
}

#[tokio::test]
async fn v2_top_level_is_notes_and_photos_prefix() {
    let server = TestServer::spawn().await;
    let client = server.client();
    seed_photos(&client).await;

    let out = client
        .list_objects_v2()
        .bucket("photos")
        .delimiter("/")
        .send()
        .await
        .unwrap();
    let cps: Vec<&str> = out
        .common_prefixes()
        .iter()
        .filter_map(|c| c.prefix())
        .collect();
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(cps, ["photos/"]);
    assert_eq!(keys, ["notes.txt"]);
}

#[tokio::test]
async fn v2_recursive_lists_all_five_in_order() {
    let server = TestServer::spawn().await;
    let client = server.client();
    seed_photos(&client).await;

    let out = client
        .list_objects_v2()
        .bucket("photos")
        .send()
        .await
        .unwrap();
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(
        keys,
        [
            "notes.txt",
            "photos/2024/a.jpg",
            "photos/2024/b.jpg",
            "photos/2025/c.jpg",
            "photos/index.md",
        ],
        "flat, lexicographic, no dir markers"
    );
    assert!(out.common_prefixes().is_empty());
    // Fields present on a content object.
    let first = &out.contents()[0];
    assert_eq!(first.storage_class().map(|s| s.as_str()), Some("STANDARD"));
    assert!(first.e_tag().unwrap().starts_with('"'), "ETag is quoted");
    assert!(first.owner().is_none(), "no Owner without fetch-owner");
}

#[tokio::test]
async fn v2_fetch_owner_includes_owner() {
    let server = TestServer::spawn().await;
    let client = server.client();
    seed_photos(&client).await;

    let out = client
        .list_objects_v2()
        .bucket("photos")
        .fetch_owner(true)
        .send()
        .await
        .unwrap();
    let owner = out.contents()[0].owner().expect("Owner present");
    assert_eq!(owner.id(), Some(common::ACCESS_KEY));
    assert_eq!(owner.display_name(), Some(common::ACCESS_KEY));
}

#[tokio::test]
async fn v2_pagination_yields_every_key_once_in_order() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "paged").await;
    let keys: Vec<String> = (0..2500).map(|i| format!("k{i:05}")).collect();
    server.seed_object_rows("paged", keys.iter().map(String::as_str));

    let mut collected: Vec<String> = Vec::new();
    let mut token: Option<String> = None;
    let mut pages = 0;
    loop {
        let mut req = client.list_objects_v2().bucket("paged").max_keys(1000);
        if let Some(t) = &token {
            req = req.continuation_token(t);
        }
        let out = req.send().await.expect("page");
        pages += 1;
        let page_keys: Vec<String> = out
            .contents()
            .iter()
            .filter_map(|o| o.key())
            .map(str::to_owned)
            .collect();
        assert!(page_keys.len() <= 1000, "page over the cap");
        collected.extend(page_keys);
        if out.is_truncated() == Some(true) {
            token = Some(
                out.next_continuation_token()
                    .expect("truncated page has a token")
                    .to_owned(),
            );
        } else {
            assert!(
                out.next_continuation_token().is_none(),
                "last page has no token"
            );
            break;
        }
    }
    assert_eq!(pages, 3, "2500 / 1000 → 3 pages");
    assert_eq!(collected.len(), 2500, "no dupes, none dropped");
    assert_eq!(collected, keys, "every key once, in order");
}

#[tokio::test]
async fn v2_max_keys_is_capped_at_1000() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "paged").await;
    server.seed_object_rows(
        "paged",
        (0..2500)
            .map(|i| format!("k{i:05}"))
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str),
    );

    let out = client
        .list_objects_v2()
        .bucket("paged")
        .max_keys(5000)
        .send()
        .await
        .unwrap();
    assert!(
        out.contents().len() <= 1000,
        "request for 5000 capped to 1000"
    );
    assert_eq!(out.contents().len(), 1000);
    assert_eq!(out.is_truncated(), Some(true));
    assert_eq!(out.max_keys(), Some(1000), "echoed MaxKeys is the cap");
}

#[tokio::test]
async fn v2_start_after_begins_strictly_after() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "paged").await;
    server.seed_object_rows(
        "paged",
        (0..2500)
            .map(|i| format!("k{i:05}"))
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str),
    );

    let out = client
        .list_objects_v2()
        .bucket("paged")
        .start_after("k01000")
        .max_keys(3)
        .send()
        .await
        .unwrap();
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys, ["k01001", "k01002", "k01003"]);
    assert_eq!(out.start_after(), Some("k01000"), "StartAfter echoed");
}

#[tokio::test]
async fn v2_negative_max_keys_is_invalid_argument() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "paged").await;
    let err = client
        .list_objects_v2()
        .bucket("paged")
        .max_keys(-1)
        .send()
        .await
        .expect_err("negative max-keys");
    assert_eq!(err.code(), Some("InvalidArgument"), "unexpected: {err:?}");
}

#[tokio::test]
async fn v2_bad_continuation_token_is_invalid_argument() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "paged").await;
    let err = client
        .list_objects_v2()
        .bucket("paged")
        .continuation_token("not*valid*base64")
        .send()
        .await
        .expect_err("bad token");
    assert_eq!(err.code(), Some("InvalidArgument"), "unexpected: {err:?}");
}

#[tokio::test]
async fn v2_no_match_prefix_is_empty_not_truncated() {
    let server = TestServer::spawn().await;
    let client = server.client();
    seed_photos(&client).await;
    let out = client
        .list_objects_v2()
        .bucket("photos")
        .prefix("nope/")
        .send()
        .await
        .unwrap();
    assert!(out.contents().is_empty());
    assert!(out.common_prefixes().is_empty());
    assert_eq!(out.key_count(), Some(0));
    assert_eq!(out.is_truncated(), Some(false));
}

#[tokio::test]
async fn v2_encoding_type_url_round_trips_a_weird_key() {
    use aws_sdk_s3::primitives::ByteStream;
    use aws_sdk_s3::types::EncodingType;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "photos").await;
    let weird = "my report (v2).txt";
    client
        .put_object()
        .bucket("photos")
        .key(weird)
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();

    let out = client
        .list_objects_v2()
        .bucket("photos")
        .encoding_type(EncodingType::Url)
        .send()
        .await
        .expect("list with encoding-type=url");

    assert_eq!(
        out.encoding_type(),
        Some(&EncodingType::Url),
        "EncodingType echoed"
    );
    // The SDK returns the raw (encoded) key from the XML; a percent-decode
    // recovers the real name, which is exactly what rclone does.
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(
        keys,
        ["my%20report%20%28v2%29.txt"],
        "key is URL-encoded on the wire"
    );
    assert_eq!(cubby::listing::url_encode(weird), keys[0]);

    // Without encoding-type the same key lists literally (encoding is
    // presentation-only; the stored key is unchanged).
    let plain = client
        .list_objects_v2()
        .bucket("photos")
        .send()
        .await
        .unwrap();
    let plain_keys: Vec<&str> = plain.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(plain_keys, [weird], "stored key untouched");
}

#[tokio::test]
async fn v2_missing_bucket_is_no_such_bucket() {
    let server = TestServer::spawn().await;
    let err = server
        .client()
        .list_objects_v2()
        .bucket("ghost")
        .send()
        .await
        .expect_err("missing bucket");
    assert_eq!(err.code(), Some("NoSuchBucket"), "unexpected: {err:?}");
}

#[tokio::test]
async fn v1_delimiter_groups_and_always_has_owner() {
    let server = TestServer::spawn().await;
    let client = server.client();
    seed_photos(&client).await;

    let out = client
        .list_objects()
        .bucket("photos")
        .delimiter("/")
        .send()
        .await
        .expect("list v1");
    let cps: Vec<&str> = out
        .common_prefixes()
        .iter()
        .filter_map(|c| c.prefix())
        .collect();
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(cps, ["photos/"]);
    assert_eq!(keys, ["notes.txt"]);
    // v1 always includes Owner.
    assert!(
        out.contents()[0].owner().is_some(),
        "v1 Contents carry Owner unconditionally"
    );
    // Not truncated (well under 1000) ⇒ no NextMarker even with a delimiter.
    assert_eq!(out.is_truncated(), Some(false));
    assert!(out.next_marker().is_none());
}

#[tokio::test]
async fn v1_next_marker_present_only_with_delimiter() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "listv1").await;
    // Five delimiter groups; force truncation at max-keys=2.
    server.seed_object_rows("listv1", ["g1/a", "g1/b", "g2/a", "g3/a", "g4/a", "g5/a"]);

    // With a delimiter: truncated page carries a resuming NextMarker.
    let mut cps = Vec::new();
    let mut marker: Option<String> = None;
    loop {
        let mut req = client
            .list_objects()
            .bucket("listv1")
            .delimiter("/")
            .max_keys(2);
        if let Some(m) = &marker {
            req = req.marker(m);
        }
        let out = req.send().await.expect("v1 page");
        cps.extend(
            out.common_prefixes()
                .iter()
                .filter_map(|c| c.prefix())
                .map(str::to_owned),
        );
        if out.is_truncated() == Some(true) {
            marker = Some(
                out.next_marker()
                    .expect("delimiter truncation → NextMarker")
                    .to_owned(),
            );
        } else {
            assert!(out.next_marker().is_none(), "final page has no NextMarker");
            break;
        }
    }
    assert_eq!(
        cps,
        ["g1/", "g2/", "g3/", "g4/", "g5/"],
        "each group once, in order"
    );

    // Without a delimiter: truncated but NextMarker is absent (client resumes
    // from the last Key itself).
    let out = client
        .list_objects()
        .bucket("listv1")
        .max_keys(2)
        .send()
        .await
        .unwrap();
    assert_eq!(out.is_truncated(), Some(true));
    assert!(
        out.next_marker().is_none(),
        "no delimiter ⇒ no NextMarker (S3 quirk)"
    );
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys, ["g1/a", "g1/b"]);
}

#[tokio::test]
async fn v1_marker_resumes_strictly_after() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "listv1").await;
    server.seed_object_rows("listv1", ["a", "b", "c", "d"]);

    let out = client
        .list_objects()
        .bucket("listv1")
        .marker("b")
        .send()
        .await
        .unwrap();
    let keys: Vec<&str> = out.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys, ["c", "d"], "marker is strictly-after");
    assert_eq!(out.marker(), Some("b"), "Marker echoed");
}

#[tokio::test]
async fn v1_missing_bucket_is_no_such_bucket() {
    let server = TestServer::spawn().await;
    let err = server
        .client()
        .list_objects()
        .bucket("ghost")
        .send()
        .await
        .expect_err("missing bucket");
    assert_eq!(err.code(), Some("NoSuchBucket"), "unexpected: {err:?}");
}

/// Send a raw HTTP/1.1 GET and return `(status_code, body)`. Dependency-free.
async fn raw_http_get(addr: std::net::SocketAddr, path: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();

    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
        .to_owned();
    (status, body)
}

// =====================================================================
// Multipart (Phase 3)
// =====================================================================

use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

/// Upload one part and return the ETag the server assigned it.
async fn upload_part(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_number: i32,
    body: &[u8],
) -> String {
    use aws_sdk_s3::primitives::ByteStream;
    let out = client
        .upload_part()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .part_number(part_number)
        .body(ByteStream::from(body.to_vec()))
        .send()
        .await
        .expect("upload-part 200");
    out.e_tag().expect("part etag present").to_owned()
}

/// Create an upload and return its id.
async fn create_upload(client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> String {
    client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .expect("create-multipart 200")
        .upload_id()
        .expect("upload id")
        .to_owned()
}

#[tokio::test]
async fn create_multipart_returns_upload_id() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;

    let out = client
        .create_multipart_upload()
        .bucket("mpbucket")
        .key("big.bin")
        .send()
        .await
        .expect("create-multipart 200");
    assert!(
        out.upload_id().map(|s| !s.is_empty()).unwrap_or(false),
        "non-empty upload id"
    );
    assert_eq!(out.bucket(), Some("mpbucket"));
    assert_eq!(out.key(), Some("big.bin"));
    // Staging dir exists on disk.
    let stage = server
        .datadir
        .multipart_dir()
        .join(out.upload_id().unwrap());
    assert!(stage.is_dir(), "staging dir created");
}

#[tokio::test]
async fn create_multipart_on_missing_bucket_is_no_such_bucket() {
    let server = TestServer::spawn().await;
    let err = server
        .client()
        .create_multipart_upload()
        .bucket("nope")
        .key("k")
        .send()
        .await
        .expect_err("must fail on absent bucket");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchBucket"));
}

#[tokio::test]
async fn upload_part_writes_bytes_and_replaces_on_reupload() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;
    let uid = create_upload(&client, "mpbucket", "big.bin").await;

    let p1 = b"AAAAAAAAAA".to_vec();
    let p2 = b"BBBBBBBBBBBBBBBBBBBB".to_vec();
    let e1 = upload_part(&client, "mpbucket", "big.bin", &uid, 1, &p1).await;
    let e2 = upload_part(&client, "mpbucket", "big.bin", &uid, 2, &p2).await;
    assert_eq!(e1, format!("\"{}\"", md5_hex(&p1)));
    assert_eq!(e2, format!("\"{}\"", md5_hex(&p2)));

    // Part files exist on disk with the right bytes.
    let stage = server.datadir.multipart_dir().join(&uid);
    assert_eq!(std::fs::read(stage.join("1")).unwrap(), p1);
    assert_eq!(std::fs::read(stage.join("2")).unwrap(), p2);

    // Re-uploading part 1 overwrites its bytes and ETag.
    let p1b = b"ZZZZ".to_vec();
    let e1b = upload_part(&client, "mpbucket", "big.bin", &uid, 1, &p1b).await;
    assert_eq!(e1b, format!("\"{}\"", md5_hex(&p1b)));
    assert_eq!(std::fs::read(stage.join("1")).unwrap(), p1b);
}

#[tokio::test]
async fn upload_part_bogus_upload_id_is_no_such_upload() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;
    let err = client
        .upload_part()
        .bucket("mpbucket")
        .key("k")
        .upload_id("deadbeefdeadbeefdeadbeefdeadbeef")
        .part_number(1)
        .body(ByteStream::from(b"x".to_vec()))
        .send()
        .await
        .expect_err("bogus upload id must fail");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchUpload"));
}

#[tokio::test]
async fn upload_part_out_of_range_number_is_invalid_argument() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;
    let uid = create_upload(&client, "mpbucket", "k").await;
    let err = client
        .upload_part()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .part_number(10001)
        .body(ByteStream::from(b"x".to_vec()))
        .send()
        .await
        .expect_err("part number 10001 must be rejected");
    assert_eq!(
        err.into_service_error().meta().code(),
        Some("InvalidArgument")
    );
}

#[tokio::test]
async fn list_parts_ascending_with_pagination() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;
    let uid = create_upload(&client, "mpbucket", "k").await;
    let p1 = b"AAAAAAAAAA".to_vec(); // 10 bytes
    let p2 = b"BBBBBBBBBBBBBBBBBBBB".to_vec(); // 20 bytes
    let e1 = upload_part(&client, "mpbucket", "k", &uid, 1, &p1).await;
    let e2 = upload_part(&client, "mpbucket", "k", &uid, 2, &p2).await;

    // Full listing: both parts ascending with correct sizes and ETags.
    let out = client
        .list_parts()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .send()
        .await
        .expect("list-parts 200");
    let parts = out.parts();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].part_number(), Some(1));
    assert_eq!(parts[0].size(), Some(10));
    assert_eq!(parts[0].e_tag(), Some(e1.as_str()));
    assert_eq!(parts[1].part_number(), Some(2));
    assert_eq!(parts[1].size(), Some(20));
    assert_eq!(parts[1].e_tag(), Some(e2.as_str()));

    // A max-parts=1 page is truncated; its marker resumes at part 2.
    let page1 = client
        .list_parts()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .max_parts(1)
        .send()
        .await
        .unwrap();
    assert_eq!(page1.is_truncated(), Some(true));
    assert_eq!(page1.parts().len(), 1);
    assert_eq!(page1.next_part_number_marker(), Some("1"));

    let page2 = client
        .list_parts()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .part_number_marker("1")
        .send()
        .await
        .unwrap();
    assert_eq!(page2.parts().len(), 1);
    assert_eq!(page2.parts()[0].part_number(), Some(2));
}

#[tokio::test]
async fn list_parts_bogus_upload_id_is_no_such_upload() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;
    let err = client
        .list_parts()
        .bucket("mpbucket")
        .key("k")
        .upload_id("deadbeefdeadbeefdeadbeefdeadbeef")
        .send()
        .await
        .expect_err("bogus upload id must fail");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchUpload"));
}

/// Drive a full 3-part upload and complete it; return the composite ETag.
async fn complete_three_parts(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    p1: &[u8],
    p2: &[u8],
    p3: &[u8],
) -> String {
    let uid = create_upload(client, bucket, key).await; // bucket passed by caller
    let e1 = upload_part(client, bucket, key, &uid, 1, p1).await;
    let e2 = upload_part(client, bucket, key, &uid, 2, p2).await;
    let e3 = upload_part(client, bucket, key, &uid, 3, p3).await;
    let completed = CompletedMultipartUpload::builder()
        .parts(CompletedPart::builder().part_number(1).e_tag(e1).build())
        .parts(CompletedPart::builder().part_number(2).e_tag(e2).build())
        .parts(CompletedPart::builder().part_number(3).e_tag(e3).build())
        .build();
    let out = client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(&uid)
        .multipart_upload(completed)
        .send()
        .await
        .expect("complete 200");
    out.e_tag().expect("composite etag").to_owned()
}

#[tokio::test]
async fn complete_assembles_one_real_file_with_composite_etag() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;

    let p1 = vec![b'a'; 100];
    let p2 = vec![b'b'; 200];
    let p3 = vec![b'c'; 50];
    let etag = complete_three_parts(&client, "mpbucket", "big.bin", &p1, &p2, &p3).await;

    // Composite ETag matches the md5-of-md5s formula, suffixed -3.
    let expected = {
        use md5::{Digest, Md5};
        let mut h = Md5::new();
        h.update(Md5::digest(&p1));
        h.update(Md5::digest(&p2));
        h.update(Md5::digest(&p3));
        format!("\"{}-3\"", hex::encode(h.finalize()))
    };
    assert_eq!(etag, expected);

    // get_object bytes == part1+part2+part3.
    let mut concat = p1.clone();
    concat.extend_from_slice(&p2);
    concat.extend_from_slice(&p3);
    let got = client
        .get_object()
        .bucket("mpbucket")
        .key("big.bin")
        .send()
        .await
        .unwrap();
    let bytes = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(bytes.as_ref(), concat.as_slice());

    // head_object size == sum of parts, and carries the composite etag.
    let head = client
        .head_object()
        .bucket("mpbucket")
        .key("big.bin")
        .send()
        .await
        .unwrap();
    assert_eq!(head.content_length(), Some(350));
    assert_eq!(head.e_tag(), Some(etag.as_str()));

    // On-disk file cmps clean against the concatenation.
    let on_disk = std::fs::read(server.object_path("mpbucket", "big.bin")).unwrap();
    assert_eq!(on_disk, concat);

    // Staging tree is gone.
    let entries: Vec<_> = std::fs::read_dir(server.datadir.multipart_dir())
        .unwrap()
        .collect();
    assert!(entries.is_empty(), "no staging dirs left after complete");
}

#[tokio::test]
async fn complete_overwrites_existing_key_last_writer_wins() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;

    // Single-PUT object at k first.
    client
        .put_object()
        .bucket("mpbucket")
        .key("k")
        .body(ByteStream::from(b"original-single-put".to_vec()))
        .send()
        .await
        .unwrap();

    // Now complete a multipart to the same key.
    let p1 = vec![b'x'; 30];
    let p2 = vec![b'y'; 40];
    let p3 = vec![b'z'; 10];
    let etag = complete_three_parts(&client, "mpbucket", "k", &p1, &p2, &p3).await;
    assert!(etag.contains("-3"));

    let mut concat = p1.clone();
    concat.extend_from_slice(&p2);
    concat.extend_from_slice(&p3);
    let on_disk = std::fs::read(server.object_path("mpbucket", "k")).unwrap();
    assert_eq!(on_disk, concat, "multipart content replaced the single-put");
}

#[tokio::test]
async fn abort_removes_staging_and_upload() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;
    let uid = create_upload(&client, "mpbucket", "k").await;
    upload_part(&client, "mpbucket", "k", &uid, 1, b"some-bytes").await;
    let stage = server.datadir.multipart_dir().join(&uid);
    assert!(stage.is_dir());

    client
        .abort_multipart_upload()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .send()
        .await
        .expect("abort 200");

    assert!(!stage.exists(), "staging dir gone after abort");

    // Subsequent list-parts → NoSuchUpload.
    let err = client
        .list_parts()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .send()
        .await
        .expect_err("list after abort must fail");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchUpload"));

    // No object row was created → head 404.
    let err = client
        .head_object()
        .bucket("mpbucket")
        .key("k")
        .send()
        .await
        .expect_err("head after abort must 404");
    assert_eq!(err.into_service_error().meta().code(), Some("NotFound"));
}

#[tokio::test]
async fn complete_error_paths() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "mpbucket").await;

    // Set up an upload with two parts.
    let uid = create_upload(&client, "mpbucket", "k").await;
    let e1 = upload_part(&client, "mpbucket", "k", &uid, 1, b"AAAAA").await;
    let e2 = upload_part(&client, "mpbucket", "k", &uid, 2, b"BBBBB").await;

    // (a) Wrong part ETag → InvalidPart.
    let bad = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag("\"00000000000000000000000000000000\"")
                .build(),
        )
        .build();
    let err = client
        .complete_multipart_upload()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .multipart_upload(bad)
        .send()
        .await
        .expect_err("wrong etag must fail");
    assert_eq!(err.into_service_error().meta().code(), Some("InvalidPart"));

    // (b) Parts out of ascending order → InvalidPartOrder.
    let descending = CompletedMultipartUpload::builder()
        .parts(CompletedPart::builder().part_number(2).e_tag(&e2).build())
        .parts(CompletedPart::builder().part_number(1).e_tag(&e1).build())
        .build();
    let err = client
        .complete_multipart_upload()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .multipart_upload(descending)
        .send()
        .await
        .expect_err("descending order must fail");
    assert_eq!(
        err.into_service_error().meta().code(),
        Some("InvalidPartOrder")
    );

    // (c) Empty part list → InvalidRequest.
    let empty = CompletedMultipartUpload::builder().build();
    let err = client
        .complete_multipart_upload()
        .bucket("mpbucket")
        .key("k")
        .upload_id(&uid)
        .multipart_upload(empty)
        .send()
        .await
        .expect_err("empty list must fail");
    assert_eq!(
        err.into_service_error().meta().code(),
        Some("InvalidRequest")
    );

    // (d) Bogus upload id on complete → NoSuchUpload.
    let one = CompletedMultipartUpload::builder()
        .parts(CompletedPart::builder().part_number(1).e_tag(&e1).build())
        .build();
    let err = client
        .complete_multipart_upload()
        .bucket("mpbucket")
        .key("k")
        .upload_id("deadbeefdeadbeefdeadbeefdeadbeef")
        .multipart_upload(one)
        .send()
        .await
        .expect_err("bogus upload id must fail");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchUpload"));
}

// --- CopyObject (Phase 4) --------------------------------------------------

#[tokio::test]
async fn copy_object_happy_path_carries_bytes_and_source_etag() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;

    let data = b"the source bytes, copied verbatim onto a new key".to_vec();
    let put = client
        .put_object()
        .bucket("bkt")
        .key("src.bin")
        .body(ByteStream::from(data.clone()))
        .send()
        .await
        .unwrap();
    let src_etag = put.e_tag().unwrap().to_owned();

    let out = client
        .copy_object()
        .bucket("bkt")
        .key("dst.bin")
        .copy_source("bkt/src.bin")
        .send()
        .await
        .expect("copy-object 200");

    // CopyObjectResult carries the (preserved) source ETag.
    let res = out
        .copy_object_result()
        .expect("copy_object_result present");
    assert_eq!(
        res.e_tag(),
        Some(src_etag.as_str()),
        "dest ETag == source ETag"
    );
    assert!(res.last_modified().is_some(), "LastModified present");

    // The copy is a real browsable file, byte-for-byte the source.
    let dst_disk = std::fs::read(server.object_path("bkt", "dst.bin")).expect("dst file exists");
    assert_eq!(dst_disk, data, "dst on-disk bytes cmp-clean against source");

    // GET returns the source bytes and the same ETag.
    let got = client
        .get_object()
        .bucket("bkt")
        .key("dst.bin")
        .send()
        .await
        .unwrap();
    assert_eq!(got.e_tag(), Some(src_etag.as_str()));
    let body = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(&body[..], &data[..]);

    // No temp files left after the staged copy.
    let leftovers: Vec<_> = std::fs::read_dir(server.datadir.tmp_dir())
        .unwrap()
        .collect();
    assert!(leftovers.is_empty(), "no temp files left after copy rename");
}

#[tokio::test]
async fn copy_object_source_key_missing_is_no_such_key() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    let err = client
        .copy_object()
        .bucket("bkt")
        .key("dst")
        .copy_source("bkt/does-not-exist")
        .send()
        .await
        .expect_err("missing source key");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchKey"));
}

#[tokio::test]
async fn copy_object_source_bucket_missing_is_no_such_bucket() {
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    let err = client
        .copy_object()
        .bucket("bkt")
        .key("dst")
        .copy_source("no-bucket/k")
        .send()
        .await
        .expect_err("missing source bucket");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchBucket"));
}

#[tokio::test]
async fn copy_object_dest_bucket_missing_is_no_such_bucket() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    client
        .put_object()
        .bucket("bkt")
        .key("src")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();
    let err = client
        .copy_object()
        .bucket("ghost")
        .key("dst")
        .copy_source("bkt/src")
        .send()
        .await
        .expect_err("missing dest bucket");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchBucket"));
    // No partial file left in a (nonexistent) dest bucket dir.
    assert!(!server.object_path("ghost", "dst").exists());
}

#[tokio::test]
async fn copy_object_copy_directive_carries_source_metadata() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    client
        .put_object()
        .bucket("bkt")
        .key("src")
        .content_type("application/json")
        .metadata("team", "x")
        .body(ByteStream::from_static(b"{}"))
        .send()
        .await
        .unwrap();

    // Default directive (COPY) carries content-type and user metadata.
    client
        .copy_object()
        .bucket("bkt")
        .key("dst")
        .copy_source("bkt/src")
        .send()
        .await
        .unwrap();

    let head = client
        .head_object()
        .bucket("bkt")
        .key("dst")
        .send()
        .await
        .unwrap();
    assert_eq!(head.content_type(), Some("application/json"));
    assert_eq!(
        head.metadata().unwrap().get("team").map(String::as_str),
        Some("x")
    );
}

#[tokio::test]
async fn copy_object_replace_directive_takes_request_metadata() {
    use aws_sdk_s3::primitives::ByteStream;
    use aws_sdk_s3::types::MetadataDirective;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    client
        .put_object()
        .bucket("bkt")
        .key("src")
        .content_type("application/json")
        .metadata("team", "x")
        .body(ByteStream::from_static(b"payload-bytes"))
        .send()
        .await
        .unwrap();

    client
        .copy_object()
        .bucket("bkt")
        .key("dst")
        .copy_source("bkt/src")
        .metadata_directive(MetadataDirective::Replace)
        .content_type("text/plain")
        .metadata("v", "2")
        .send()
        .await
        .unwrap();

    let head = client
        .head_object()
        .bucket("bkt")
        .key("dst")
        .send()
        .await
        .unwrap();
    assert_eq!(
        head.content_type(),
        Some("text/plain"),
        "content-type from request"
    );
    let md = head.metadata().unwrap();
    assert_eq!(
        md.get("v").map(String::as_str),
        Some("2"),
        "metadata from request"
    );
    assert!(
        !md.contains_key("team"),
        "source metadata not carried on REPLACE"
    );
    // Bytes still copied.
    assert_eq!(
        std::fs::read(server.object_path("bkt", "dst")).unwrap(),
        b"payload-bytes"
    );
}

#[tokio::test]
async fn copy_object_same_key_replace_is_metadata_only() {
    use aws_sdk_s3::primitives::ByteStream;
    use aws_sdk_s3::types::MetadataDirective;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    let original = b"the original object bytes must be untouched".to_vec();
    client
        .put_object()
        .bucket("bkt")
        .key("k")
        .content_type("application/octet-stream")
        .body(ByteStream::from(original.clone()))
        .send()
        .await
        .unwrap();

    client
        .copy_object()
        .bucket("bkt")
        .key("k")
        .copy_source("bkt/k")
        .metadata_directive(MetadataDirective::Replace)
        .content_type("text/plain")
        .metadata("v", "2")
        .send()
        .await
        .expect("same-key REPLACE is legal");

    let head = client
        .head_object()
        .bucket("bkt")
        .key("k")
        .send()
        .await
        .unwrap();
    assert_eq!(head.content_type(), Some("text/plain"));
    assert_eq!(
        head.metadata().unwrap().get("v").map(String::as_str),
        Some("2")
    );
    // Bytes untouched by the metadata-only update.
    assert_eq!(
        std::fs::read(server.object_path("bkt", "k")).unwrap(),
        original
    );
}

#[tokio::test]
async fn copy_object_same_key_copy_is_invalid_request() {
    use aws_sdk_s3::primitives::ByteStream;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    client
        .put_object()
        .bucket("bkt")
        .key("k")
        .body(ByteStream::from_static(b"x"))
        .send()
        .await
        .unwrap();
    let err = client
        .copy_object()
        .bucket("bkt")
        .key("k")
        .copy_source("bkt/k")
        .send()
        .await
        .expect_err("self-copy with default directive is illegal");
    assert_eq!(
        err.into_service_error().meta().code(),
        Some("InvalidRequest")
    );
}

// --- DeleteObjects (batch, Phase 4) ----------------------------------------

async fn put_bytes(client: &aws_sdk_s3::Client, bucket: &str, key: &str, body: &'static [u8]) {
    use aws_sdk_s3::primitives::ByteStream;
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from_static(body))
        .send()
        .await
        .unwrap();
}

fn object_id(key: &str) -> aws_sdk_s3::types::ObjectIdentifier {
    aws_sdk_s3::types::ObjectIdentifier::builder()
        .key(key)
        .build()
        .unwrap()
}

#[tokio::test]
async fn delete_objects_removes_rows_and_files_and_lists_all() {
    use aws_sdk_s3::types::Delete;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    for k in ["k1", "k2", "k3"] {
        put_bytes(&client, "bkt", k, b"data").await;
    }

    let del = Delete::builder()
        .objects(object_id("k1"))
        .objects(object_id("k2"))
        .objects(object_id("k3"))
        .build()
        .unwrap();
    let out = client
        .delete_objects()
        .bucket("bkt")
        .delete(del)
        .send()
        .await
        .expect("delete-objects 200");

    // Every requested key is echoed under Deleted.
    let mut deleted: Vec<&str> = out.deleted().iter().filter_map(|d| d.key()).collect();
    deleted.sort();
    assert_eq!(deleted, ["k1", "k2", "k3"]);
    assert!(out.errors().is_empty(), "no per-key errors");

    // Files gone on disk.
    for k in ["k1", "k2", "k3"] {
        assert!(!server.object_path("bkt", k).exists(), "{k} unlinked");
    }
    // And gone from listing.
    let list = client.list_objects_v2().bucket("bkt").send().await.unwrap();
    assert!(
        list.contents().is_empty(),
        "listing empty after batch delete"
    );
}

#[tokio::test]
async fn delete_objects_is_idempotent_for_missing_keys() {
    use aws_sdk_s3::types::Delete;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    put_bytes(&client, "bkt", "real", b"x").await;

    let del = Delete::builder()
        .objects(object_id("real"))
        .objects(object_id("ghost"))
        .build()
        .unwrap();
    let out = client
        .delete_objects()
        .bucket("bkt")
        .delete(del)
        .send()
        .await
        .expect("delete-objects 200 even with a never-existed key");
    let mut deleted: Vec<&str> = out.deleted().iter().filter_map(|d| d.key()).collect();
    deleted.sort();
    assert_eq!(
        deleted,
        ["ghost", "real"],
        "missing key reported deleted too"
    );
    assert!(out.errors().is_empty());
}

#[tokio::test]
async fn delete_objects_quiet_mode_returns_no_deleted_entries() {
    use aws_sdk_s3::types::Delete;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    for k in ["k1", "k2"] {
        put_bytes(&client, "bkt", k, b"x").await;
    }

    let del = Delete::builder()
        .objects(object_id("k1"))
        .objects(object_id("k2"))
        .quiet(true)
        .build()
        .unwrap();
    let out = client
        .delete_objects()
        .bucket("bkt")
        .delete(del)
        .send()
        .await
        .expect("quiet delete-objects 200");
    assert!(out.deleted().is_empty(), "quiet mode: no Deleted entries");
    assert!(
        out.errors().is_empty(),
        "quiet mode: no Errors on full success"
    );
    // Keys still removed from disk.
    for k in ["k1", "k2"] {
        assert!(
            !server.object_path("bkt", k).exists(),
            "{k} removed in quiet mode"
        );
    }
}

#[tokio::test]
async fn delete_objects_on_missing_bucket_is_no_such_bucket() {
    use aws_sdk_s3::types::Delete;
    let server = TestServer::spawn().await;
    let client = server.client();
    let del = Delete::builder().objects(object_id("k")).build().unwrap();
    let err = client
        .delete_objects()
        .bucket("ghost")
        .delete(del)
        .send()
        .await
        .expect_err("delete-objects on a missing bucket");
    assert_eq!(err.into_service_error().meta().code(), Some("NoSuchBucket"));
}

#[tokio::test]
async fn delete_objects_over_1000_is_invalid_request() {
    use aws_sdk_s3::types::Delete;
    let server = TestServer::spawn().await;
    let client = server.client();
    make_bucket(&client, "bkt").await;
    let mut builder = Delete::builder();
    for i in 0..1001 {
        builder = builder.objects(object_id(&format!("k{i}")));
    }
    let del = builder.build().unwrap();
    let err = client
        .delete_objects()
        .bucket("bkt")
        .delete(del)
        .send()
        .await
        .expect_err("more than 1000 keys must be rejected");
    assert_eq!(
        err.into_service_error().meta().code(),
        Some("InvalidRequest")
    );
}

// ---- --seed ----------------------------------------------------------------
//
// Inner-loop tests for `seed::apply`, driven through a real server seeded from
// the committed example `seed.yaml` (its `file:` fixture resolves against the
// repo root). The outer loop is the AWS CLI acceptance boxes.

/// The committed example seed file at the repo root.
fn example_seed() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("seed.yaml")
}

#[tokio::test]
async fn seed_creates_buckets_and_inline_object() {
    let server = TestServer::spawn_seeded(&example_seed()).await;
    let client = server.client();

    // Both declared buckets exist to S3 and on disk (`reports` has no objects).
    let out = client.list_buckets().send().await.unwrap();
    let mut names: Vec<&str> = out.buckets().iter().filter_map(|b| b.name()).collect();
    names.sort();
    assert_eq!(names, ["reports", "uploads"]);
    assert!(server.datadir.bucket_dir("uploads").is_dir());
    assert!(server.datadir.bucket_dir("reports").is_dir());

    // The inline object is a real cat-able file with the exact bytes…
    let on_disk = std::fs::read(server.object_path("uploads", "hello.txt")).unwrap();
    assert_eq!(on_disk, b"hi there\n");

    // …and GET returns those bytes with the content-MD5 ETag of a client PUT.
    let got = client
        .get_object()
        .bucket("uploads")
        .key("hello.txt")
        .send()
        .await
        .unwrap();
    assert_eq!(
        got.e_tag(),
        Some(format!("\"{}\"", md5_hex(b"hi there\n")).as_str())
    );
    let body = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(body.as_ref(), b"hi there\n");
}

#[tokio::test]
async fn seed_loads_file_backed_object() {
    let server = TestServer::spawn_seeded(&example_seed()).await;
    let client = server.client();

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/logo.png");
    let expected = std::fs::read(&fixture).unwrap();

    // The on-disk file is cmp-clean against the fixture (real bytes loaded).
    let on_disk = std::fs::read(server.object_path("uploads", "photos/logo.png")).unwrap();
    assert_eq!(on_disk, expected, "seeded file bytes match the fixture");

    // And GET returns the identical bytes.
    let got = client
        .get_object()
        .bucket("uploads")
        .key("photos/logo.png")
        .send()
        .await
        .unwrap();
    let body = got.body.collect().await.unwrap().into_bytes();
    assert_eq!(body.as_ref(), expected.as_slice());
}

#[tokio::test]
async fn seed_applies_content_type_and_metadata() {
    let server = TestServer::spawn_seeded(&example_seed()).await;
    let client = server.client();

    let head = client
        .head_object()
        .bucket("uploads")
        .key("hello.txt")
        .send()
        .await
        .unwrap();
    assert_eq!(head.content_type(), Some("text/plain"));
    assert_eq!(
        head.metadata()
            .and_then(|m| m.get("team"))
            .map(String::as_str),
        Some("platform")
    );

    // The file-backed object carried its declared content type too.
    let head_png = client
        .head_object()
        .bucket("uploads")
        .key("photos/logo.png")
        .send()
        .await
        .unwrap();
    assert_eq!(head_png.content_type(), Some("image/png"));
}

#[tokio::test]
async fn seed_rerun_is_idempotent_and_declarative() {
    use cubby::datadir::DataDir;
    use cubby::db::Db;
    use cubby::store::Store;

    /// A one-shot streaming body over the given bytes (for the out-of-band PUT).
    fn blob(bytes: &'static [u8]) -> s3s::dto::StreamingBlob {
        s3s::dto::StreamingBlob::wrap(futures::stream::once(async move {
            Ok::<_, std::io::Error>(bytes::Bytes::from_static(bytes))
        }))
    }

    let tmp = tempfile::tempdir().unwrap();
    let datadir = DataDir::new(tmp.path().join("s3data"));
    datadir.bootstrap().unwrap();
    let db = Db::open(&datadir.meta_db_path()).unwrap();
    let store = Store::new(db.clone(), datadir.clone(), "local".to_owned());

    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = seed_dir.path().join("seed.yaml");
    let hello = datadir.bucket_dir("uploads").join("hello.txt");

    // First apply.
    std::fs::write(
        &seed_path,
        "buckets:\n  - name: uploads\n    objects:\n      - key: hello.txt\n        content: \"one\"\n",
    )
    .unwrap();
    cubby::seed::apply(&seed_path, &store).await.unwrap();
    assert_eq!(std::fs::read(&hello).unwrap(), b"one");

    // Re-applying the same seed succeeds — no "bucket already exists" error.
    cubby::seed::apply(&seed_path, &store).await.unwrap();
    assert_eq!(std::fs::read(&hello).unwrap(), b"one");

    // A key written out-of-band (not named by the seed) …
    store
        .put_bytes(
            "uploads",
            "manual.txt",
            Some(blob(b"manual")),
            None,
            "{}".to_owned(),
        )
        .await
        .expect("out-of-band put");

    // …survives a re-serve of an edited seed, whose named key now overwrites.
    std::fs::write(
        &seed_path,
        "buckets:\n  - name: uploads\n    objects:\n      - key: hello.txt\n        content: \"two\"\n",
    )
    .unwrap();
    cubby::seed::apply(&seed_path, &store).await.unwrap();
    assert_eq!(
        std::fs::read(&hello).unwrap(),
        b"two",
        "named key overwritten"
    );
    assert_eq!(
        std::fs::read(datadir.bucket_dir("uploads").join("manual.txt")).unwrap(),
        b"manual",
        "out-of-band key left untouched"
    );
}

/// Run `serve()` with the given seed against a fresh temp dir on `port`, and
/// return its result. Used to prove a bad seed aborts startup *before* binding.
async fn serve_with_seed(port: u16, seed_path: std::path::PathBuf) -> anyhow::Result<()> {
    use cubby::datadir::DataDir;
    use cubby::db::Db;
    use cubby::events::EventBus;
    use cubby::http::{serve, ServeConfig};

    let tmp = tempfile::tempdir().unwrap();
    let datadir = DataDir::new(tmp.path());
    datadir.bootstrap().unwrap();
    let db = Db::open(&datadir.meta_db_path()).unwrap();
    serve(ServeConfig {
        bind: "127.0.0.1".to_owned(),
        port,
        access_key: common::ACCESS_KEY.to_owned(),
        secret_key: common::SECRET_KEY.to_owned(),
        datadir,
        db,
        events: EventBus::new(),
        quiet: true,
        seed: Some(seed_path),
    })
    .await
}

#[tokio::test]
async fn seed_malformed_fails_fast_without_binding() {
    // Reserve a currently-free port, then release it so `serve()` may try to
    // bind it — which it must NOT reach, because the seed is applied first.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = seed_dir.path().join("bad.yaml");
    std::fs::write(&seed_path, "buckets:\n  - name: [not, a, string\n").unwrap();

    let err = serve_with_seed(port, seed_path)
        .await
        .expect_err("malformed seed must abort startup");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("seed") || msg.to_lowercase().contains("yaml"),
        "error should name the seed/YAML problem: {msg}"
    );

    // Nothing is listening on the port — a connect is refused.
    let conn = tokio::net::TcpStream::connect(("127.0.0.1", port)).await;
    assert!(
        conn.is_err(),
        "port {port} must be unbound after a failed seed"
    );
}

#[tokio::test]
async fn seed_missing_file_fails_fast() {
    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = seed_dir.path().join("seed.yaml");
    std::fs::write(
        &seed_path,
        "buckets:\n  - name: uploads\n    objects:\n      - key: k\n        file: ./nope.bin\n",
    )
    .unwrap();

    let err = serve_with_seed(0, seed_path)
        .await
        .expect_err("a missing file: must abort startup");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("nope.bin") || msg.to_lowercase().contains("no such file"),
        "error should name the unreadable file: {msg}"
    );
}
