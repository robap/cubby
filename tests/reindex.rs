//! Inner-loop tests for `reindex::run`, driven through a real in-process server
//! spawned against the *same* data dir the reindex populated — the proxy for the
//! AWS CLI acceptance boxes in `tests/acceptance/reindex.sh`.
//!
//! The pattern: hand-build a byte tree under `buckets/` (no rows), `reindex`,
//! then serve and ask the S3 client what it sees. reindex's only output is
//! SQLite state, so serving is how we observe it.

mod common;

use common::TestServer;

fn md5_hex(data: &[u8]) -> String {
    use md5::{Digest, Md5};
    hex::encode(Md5::digest(data))
}

/// Write a file at `buckets/<bucket>/<relpath>` (creating parents), with no
/// `objects` row — a hand-dropped file for reindex to adopt.
fn drop_file(datadir: &cubby::datadir::DataDir, bucket: &str, relpath: &str, bytes: &[u8]) {
    let path = datadir.bucket_dir(bucket).join(relpath);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[tokio::test]
async fn adopts_buckets_and_indexes_new_files() {
    const REPORT: &[u8] = b"%PDF-1.4 fake report bytes\n";
    const CAT: &[u8] = b"\x89PNG not-really pretend jpg bytes";
    const X: &[u8] = b"hello from a hand-made bucket";

    let (server, report) = TestServer::spawn_reindexed(|dd| {
        // Two files in an (unadopted) bucket, one nested; plus a whole new
        // bucket directory holding a single file.
        drop_file(dd, "uploads", "report.pdf", REPORT);
        drop_file(dd, "uploads", "photos/cat.jpg", CAT);
        drop_file(dd, "newbucket", "x", X);
    })
    .await;

    // Both bucket dirs had no row → both adopted; all three files indexed.
    assert_eq!(report.buckets_adopted, 2);
    assert_eq!(report.buckets_present, 0);
    assert_eq!(report.objects_indexed, 3);
    assert_eq!(report.objects_present, 0);
    assert_eq!(report.objects_skipped, 0);

    let client = server.client();

    // The hand-made bucket shows up in ListBuckets.
    let buckets = client.list_buckets().send().await.unwrap();
    let mut names: Vec<&str> = buckets.buckets().iter().filter_map(|b| b.name()).collect();
    names.sort();
    assert_eq!(names, ["newbucket", "uploads"]);

    // Each adopted file lists with its byte size and a single-part MD5 ETag.
    for (bucket, key, bytes) in [
        ("uploads", "report.pdf", REPORT),
        ("uploads", "photos/cat.jpg", CAT),
        ("newbucket", "x", X),
    ] {
        let head = client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .unwrap_or_else(|e| panic!("head {bucket}/{key}: {e}"));
        assert_eq!(head.content_length(), Some(bytes.len() as i64));
        assert_eq!(
            head.e_tag(),
            Some(format!("\"{}\"", md5_hex(bytes)).as_str()),
            "ETag for {bucket}/{key} must be the content-MD5"
        );

        // And the bytes round-trip on GET.
        let got = client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .unwrap();
        let body = got.body.collect().await.unwrap().into_bytes();
        assert_eq!(body.as_ref(), bytes, "GET body for {bucket}/{key}");
    }

    // The nested prefix recovered as a `/`-joined key.
    let listed = client
        .list_objects_v2()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    let mut keys: Vec<&str> = listed.contents().iter().filter_map(|o| o.key()).collect();
    keys.sort();
    assert_eq!(keys, ["photos/cat.jpg", "report.pdf"]);
}

#[tokio::test]
async fn skip_existing_is_non_destructive_and_counts_are_real() {
    // A normally-served dir with one client-PUT object carrying a real
    // content-type + user metadata, and one hand-dropped file with no row.
    let server = TestServer::spawn().await;
    let client = server.client();
    client
        .create_bucket()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    client
        .put_object()
        .bucket("uploads")
        .key("keep.txt")
        .content_type("application/x-custom")
        .metadata("team", "platform")
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"kept"))
        .send()
        .await
        .unwrap();
    // Drop a brand-new file by hand (no row) into the same bucket.
    drop_file(&server.datadir, "uploads", "fresh.txt", b"new bytes");

    // Reindex the *same* dir (a second WAL connection, like the harness's own
    // seed_object_rows helper opens).
    let db = cubby::db::Db::open(&server.datadir.meta_db_path()).unwrap();
    let report = cubby::reindex::run(&server.datadir, &db).unwrap();

    // The bucket already had a row → present, not adopted. The PUT object is
    // already indexed → present; only the hand-dropped file is newly indexed.
    assert_eq!(report.buckets_adopted, 0);
    assert_eq!(report.buckets_present, 1);
    assert_eq!(report.objects_indexed, 1);
    assert_eq!(report.objects_present, 1);

    // The pre-existing object's real content-type and metadata survive — reindex
    // did not overwrite the row with an extension guess and empty metadata.
    let head = client
        .head_object()
        .bucket("uploads")
        .key("keep.txt")
        .send()
        .await
        .unwrap();
    assert_eq!(head.content_type(), Some("application/x-custom"));
    assert_eq!(
        head.metadata()
            .and_then(|m| m.get("team"))
            .map(String::as_str),
        Some("platform")
    );

    // The hand-dropped file got the extension guess + empty metadata.
    let head_fresh = client
        .head_object()
        .bucket("uploads")
        .key("fresh.txt")
        .send()
        .await
        .unwrap();
    assert_eq!(head_fresh.content_type(), Some("text/plain"));
    assert!(head_fresh.metadata().is_none_or(|m| m.is_empty()));

    // A second run over the now-unchanged tree is a clean no-op: nothing adopted,
    // nothing indexed (convergence).
    let report2 = cubby::reindex::run(&server.datadir, &db).unwrap();
    assert_eq!(report2.buckets_adopted, 0);
    assert_eq!(report2.objects_indexed, 0);
    assert_eq!(report2.objects_present, 2);
}

#[tokio::test]
async fn skips_symlinks_and_loose_files_under_buckets() {
    let (server, report) = TestServer::spawn_reindexed(|dd| {
        drop_file(dd, "uploads", "real.txt", b"real");
        // A loose file directly under buckets/ belongs to no bucket.
        std::fs::write(dd.buckets_dir().join("loose.txt"), b"loose").unwrap();
        // A symlink inside a bucket must not be followed.
        let target = dd.bucket_dir("uploads").join("real.txt");
        let link = dd.bucket_dir("uploads").join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
    })
    .await;

    // Only the one real file is indexed; the loose file and the symlink are
    // skipped (2 on unix, 1 elsewhere where the symlink isn't created).
    assert_eq!(report.objects_indexed, 1);
    assert_eq!(report.buckets_adopted, 1);
    #[cfg(unix)]
    assert_eq!(report.objects_skipped, 2);

    // The listing shows only the real object — no `loose.txt`, no `link.txt`.
    let listed = server
        .client()
        .list_objects_v2()
        .bucket("uploads")
        .send()
        .await
        .unwrap();
    let keys: Vec<&str> = listed.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys, ["real.txt"]);
}
