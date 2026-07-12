//! `--seed`: declare buckets and fixture objects in a YAML file and have them
//! exist the instant the server is up.
//!
//! A seeded object is written through the **Phase 1 write path**
//! ([`Store::put_bytes`](crate::store::Store::put_bytes)) — temp→fsync→rename→
//! SQLite row — so it is a real browsable file at `buckets/<b>/<key>` with a
//! correct content-MD5 ETag, indistinguishable from a client `PUT` (*the
//! filesystem is the API too*). Seeding runs after `bootstrap()`/`Db::open` but
//! **before the port binds**: any error returns `Err`, so a broken fixture
//! exits non-zero and never looks like a running server.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::store::Store;

/// The whole seed file: an ordered list of buckets.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedFile {
    /// Buckets to create, in file order.
    #[serde(default)]
    pub buckets: Vec<SeedBucket>,
}

/// One bucket to create, plus the objects to write into it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedBucket {
    /// Bucket name (created if missing; a present bucket is left as-is).
    pub name: String,
    /// Objects to write into this bucket, in file order.
    #[serde(default)]
    pub objects: Vec<SeedObject>,
}

/// One object to seed. Its bytes come from **exactly one** of `content`
/// (inline UTF-8) or `file` (raw bytes from disk).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedObject {
    /// Canonical S3 key (may contain `/` for nested prefixes).
    pub key: String,
    /// Inline UTF-8 literal content. Mutually exclusive with `file`.
    #[serde(default)]
    pub content: Option<String>,
    /// Path to a local file whose raw bytes are loaded, relative to the seed
    /// file's own directory. Mutually exclusive with `content`.
    #[serde(default)]
    pub file: Option<PathBuf>,
    /// Optional content type (defaults like `PutObject` when omitted).
    #[serde(default)]
    pub content_type: Option<String>,
    /// Optional user metadata (`x-amz-meta-*`), a string→string map.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Where an object's bytes come from, after validating exactly-one-of.
enum ObjectSource<'a> {
    /// Inline UTF-8 literal.
    Inline(&'a str),
    /// A path on disk (already resolved against the seed file's directory).
    File(&'a Path),
}

impl SeedObject {
    /// Resolve the object's byte source, enforcing exactly one of
    /// `content`/`file`. A key with neither or both is a seed error.
    fn source(&self) -> anyhow::Result<ObjectSource<'_>> {
        match (&self.content, &self.file) {
            (Some(c), None) => Ok(ObjectSource::Inline(c)),
            (None, Some(f)) => Ok(ObjectSource::File(f)),
            (Some(_), Some(_)) => {
                bail!("object {:?} declares both `content` and `file`", self.key)
            }
            (None, None) => {
                bail!(
                    "object {:?} declares neither `content` nor `file`",
                    self.key
                )
            }
        }
    }
}

/// Parse and validate seed YAML: rejects unknown fields, and rejects any object
/// that does not declare exactly one of `content`/`file`.
pub fn parse(yaml: &str) -> anyhow::Result<SeedFile> {
    let file: SeedFile = serde_norway::from_str(yaml).context("parsing seed YAML")?;
    // Validate exactly-one-of up front, so a structurally broken fixture is
    // rejected before any bytes are written.
    for bucket in &file.buckets {
        for object in &bucket.objects {
            object.source()?;
        }
    }
    Ok(file)
}

/// `s3s::S3Error` is `Display` but not `std::error::Error`, so lift it into an
/// `anyhow::Error` (preserving the message) before adding seed context.
fn s3_err(e: s3s::S3Error) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// Apply a seed file to a live [`Store`]: create each bucket (idempotent) and
/// overwrite each object through the Phase 1 write path. `path` is the seed
/// file itself; `file:` object paths resolve against its directory.
///
/// Runs after `bootstrap()`/`Db::open` but **before the port binds**; returning
/// `Err` aborts startup with a non-zero exit and nothing listening. A
/// half-applied seed (some fixtures written before a later error) is acceptable
/// for a dev tool — the loud exit is what matters.
pub async fn apply(path: &Path, store: &Store) -> anyhow::Result<()> {
    let yaml = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("reading seed file {}", path.display()))?;
    let seed = parse(&yaml)?;

    // `file:` paths resolve against the seed file's own directory.
    let base = path.parent().unwrap_or_else(|| Path::new("."));

    for bucket in &seed.buckets {
        store
            .create_bucket_if_missing(&bucket.name)
            .await
            .map_err(s3_err)
            .with_context(|| format!("creating seed bucket {:?}", bucket.name))?;

        for object in &bucket.objects {
            // Build a streaming body from the object's source, buffering neither
            // inline content nor a `file:` fixture. A `file:` is opened here (not
            // inside the stream) so a missing/unreadable file fails fast with a
            // message naming the path, before any bytes are written.
            let body = match object.source()? {
                ObjectSource::Inline(c) => {
                    let bytes = bytes::Bytes::copy_from_slice(c.as_bytes());
                    s3s::dto::StreamingBlob::wrap(futures::stream::once(async move {
                        Ok::<_, std::io::Error>(bytes)
                    }))
                }
                ObjectSource::File(rel) => {
                    let resolved = base.join(rel);
                    let file = tokio::fs::File::open(&resolved).await.with_context(|| {
                        format!(
                            "reading seed object {:?} from {}",
                            object.key,
                            resolved.display()
                        )
                    })?;
                    s3s::dto::StreamingBlob::wrap(tokio_util::io::ReaderStream::new(file))
                }
            };
            // A seeded object is written through the same path as a client PUT;
            // metadata serializes to the same JSON-object shape PutObject stores.
            let metadata_json = serde_json::to_string(&object.metadata)
                .context("serializing seed object metadata")?;
            store
                .put_bytes(
                    &bucket.name,
                    &object.key,
                    Some(body),
                    object.content_type.clone(),
                    metadata_json,
                )
                .await
                .map_err(s3_err)
                .with_context(|| format!("seeding object {:?} in {:?}", object.key, bucket.name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed example must always parse — it's what the README points at.
    #[test]
    fn committed_example_parses() {
        let seed = parse(include_str!("../seed.yaml")).expect("example seed.yaml parses");
        let names: Vec<&str> = seed.buckets.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["uploads", "reports"]);
        // `reports` has no objects; `uploads` has hello.txt (inline) + a file:.
        assert!(seed.buckets[1].objects.is_empty());
        let uploads = &seed.buckets[0];
        assert_eq!(uploads.objects[0].key, "hello.txt");
        assert_eq!(uploads.objects[0].content.as_deref(), Some("hi there\n"));
        assert_eq!(
            uploads.objects[0].content_type.as_deref(),
            Some("text/plain")
        );
        assert_eq!(
            uploads.objects[0].metadata.get("team").map(String::as_str),
            Some("platform")
        );
        assert_eq!(uploads.objects[1].key, "photos/logo.png");
        assert!(uploads.objects[1].file.is_some());
    }

    #[test]
    fn bucket_without_objects_is_allowed() {
        let seed = parse("buckets:\n  - name: reports\n").unwrap();
        assert_eq!(seed.buckets.len(), 1);
        assert!(seed.buckets[0].objects.is_empty());
    }

    #[test]
    fn unknown_field_is_rejected() {
        let err = parse(
            "buckets:\n  - name: uploads\n    objects:\n      - key: k\n        content: hi\n        bogus: 1\n",
        )
        .expect_err("unknown field must be rejected");
        assert!(
            err.to_string().contains("parsing seed YAML") || format!("{err:#}").contains("bogus"),
            "error should mention the parse failure: {err:#}"
        );
    }

    #[test]
    fn object_with_both_content_and_file_is_rejected() {
        let err = parse(
            "buckets:\n  - name: uploads\n    objects:\n      - key: k\n        content: hi\n        file: ./x\n",
        )
        .expect_err("both content and file must be rejected");
        assert!(
            err.to_string().contains("both"),
            "error should name the both-declared problem: {err}"
        );
    }

    #[test]
    fn object_with_neither_content_nor_file_is_rejected() {
        let err = parse("buckets:\n  - name: uploads\n    objects:\n      - key: k\n")
            .expect_err("neither content nor file must be rejected");
        assert!(
            err.to_string().contains("neither"),
            "error should name the neither-declared problem: {err}"
        );
    }
}
