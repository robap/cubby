//! End-to-end tests driving the real `aws-sdk-s3` client against an in-process
//! buckit server — the inner-loop proxy for the AWS CLI acceptance criteria.

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

    let data = b"hello buckit, these are real cat-able bytes on disk".to_vec();
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
async fn underscore_prefix_returns_501_placeholder() {
    let server = TestServer::spawn().await;
    // A plain HTTP GET (no SigV4) — the routing layer intercepts before auth.
    let (status, body) = raw_http_get(server.addr, "/_/").await;
    assert_eq!(status, 501, "/_/ must be the UI placeholder");
    assert!(body.contains("Phase 5"), "placeholder body: {body}");
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
