//! The S3 backend: ties SQLite (source of truth for existence/metadata) to the
//! filesystem (bytes). Implements the `s3s` [`S3`] trait.
//!
//! rusqlite is synchronous, so every database call runs inside
//! `spawn_blocking`. Streaming object I/O (added from box 8) uses async
//! `tokio::fs` directly.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use s3s::dto::{
    AbortMultipartUploadInput, AbortMultipartUploadOutput, Bucket, CORSRule, CommonPrefix,
    CompleteMultipartUploadInput, CompleteMultipartUploadOutput, CopyObjectInput, CopyObjectOutput,
    CopyObjectResult, CopySource, CreateBucketInput, CreateBucketOutput,
    CreateMultipartUploadInput, CreateMultipartUploadOutput, DeleteBucketCorsInput,
    DeleteBucketCorsOutput, DeleteBucketInput, DeleteBucketOutput, DeleteObjectInput,
    DeleteObjectOutput, DeleteObjectsInput, DeleteObjectsOutput, DeletedObject, ETag, EncodingType,
    GetBucketCorsInput, GetBucketCorsOutput, GetObjectInput, GetObjectOutput, HeadBucketInput,
    HeadBucketOutput, HeadObjectInput, HeadObjectOutput, ListBucketsInput, ListBucketsOutput,
    ListObjectsInput, ListObjectsOutput, ListObjectsV2Input, ListObjectsV2Output, ListPartsInput,
    ListPartsOutput, Metadata, MetadataDirective, Object, ObjectStorageClass, Owner, Part,
    PutBucketCorsInput, PutBucketCorsOutput, PutObjectInput, PutObjectOutput, StorageClass,
    StreamingBlob, Timestamp, UploadPartInput, UploadPartOutput,
};
use s3s::{s3_error, S3Request, S3Response, S3Result};

use crate::datadir::DataDir;
use crate::db::{Db, DeleteBucketOutcome, MultipartRow, ObjectRow, PartRow};
use crate::listing::{self, ListPage, ListParams};
use crate::multipart::{self, CompleteError, RecordedPart, SubmittedPart};
use crate::notify::{EventKind, Notifier, ObjectEvent};

/// Monotonic counter for unique temp-file names within this process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// S3's default content type when a client supplies none.
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

/// The filesystem + SQLite object store.
#[derive(Clone)]
pub struct Store {
    db: Db,
    dirs: DataDir,
    /// The configured access key, used as the fixed dev `Owner` identity in
    /// listings (we have no real IAM; a stable owner keeps SDKs happy).
    access_key: String,
    /// The webhook notifier, attached only to the router-built store (via
    /// [`Store::with_notifier`]). The seed-path store leaves this `None`, so
    /// seed writes fire nothing — exactly the spec's "seed writes don't fire".
    notifier: Option<Notifier>,
}

impl Store {
    pub fn new(db: Db, dirs: DataDir, access_key: String) -> Self {
        Self {
            db,
            dirs,
            access_key,
            notifier: None,
        }
    }

    /// Attach a [`Notifier`] so successful object mutations fire webhooks. Only
    /// the router-built store gets one; seeding and tests keep the default
    /// (`None`) and stay silent.
    pub fn with_notifier(mut self, notifier: Notifier) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// Fire an object event on the attached notifier, if any. A no-op when no
    /// notifier is attached (seed/test stores). Delivery is fully async — this
    /// never blocks the mutation that called it.
    fn fire(&self, event: ObjectEvent) {
        if let Some(notifier) = &self.notifier {
            notifier.notify(event);
        }
    }

    /// Stream a request body to a fresh temp file in `.tmp/`, hashing MD5
    /// incrementally (never buffering the whole object), then flush + fsync.
    /// Returns `(temp_path, size, hex_md5)`. The temp file is removed on error.
    async fn stream_to_temp(
        &self,
        body: Option<StreamingBlob>,
    ) -> S3Result<(PathBuf, i64, String)> {
        use futures::StreamExt;
        use md5::{Digest, Md5};
        use tokio::io::AsyncWriteExt;

        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = self
            .dirs
            .tmp_dir()
            .join(format!("{}-{n}.tmp", std::process::id()));
        let mut file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(internal)?;

        let mut hasher = Md5::new();
        let mut size: i64 = 0;
        let write_result: S3Result<()> = async {
            if let Some(mut stream) = body {
                while let Some(chunk) = stream.next().await {
                    let bytes = chunk.map_err(internal)?;
                    hasher.update(&bytes);
                    file.write_all(&bytes).await.map_err(internal)?;
                    size += bytes.len() as i64;
                }
            }
            file.flush().await.map_err(internal)?;
            file.sync_all().await.map_err(internal)?; // durability before rename
            Ok(())
        }
        .await;

        if let Err(err) = write_result {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(err);
        }
        Ok((temp_path, size, hex::encode(hasher.finalize())))
    }

    /// Write `body`'s bytes as the object `bucket/<key>` through the Phase 1
    /// write path: stream into `.tmp/` (hashing the content-MD5 incrementally,
    /// never buffering the whole object), fsync, atomically rename into
    /// `buckets/<b>/<key>`, then write the authoritative SQLite row **last** —
    /// so a crash between rename and insert leaves only a harmless orphan file.
    /// Returns the hex content-MD5 (the single-part ETag).
    ///
    /// `content_type` defaults to `application/octet-stream` when `None`, and
    /// `metadata_json` is the user metadata already serialized to a JSON object
    /// (`{}` for none), exactly as [`put_object`](Self::put_object) stores it.
    ///
    /// Returns `(hex content-MD5, size in bytes)` — the caller needs the size to
    /// carry in a creation notification without re-reading the row.
    ///
    /// The bucket is assumed to already exist: PutObject verifies it up front
    /// before accepting bytes, and the `--seed` loader creates it first. This is
    /// the single write path shared by PutObject and seeding — no second,
    /// drifting implementation. It does **not** fire notifications itself (seed
    /// uses it directly); the PutObject handler fires after it returns.
    pub async fn put_bytes(
        &self,
        bucket: &str,
        key: &str,
        body: Option<StreamingBlob>,
        content_type: Option<String>,
        metadata_json: String,
    ) -> S3Result<(String, i64)> {
        let final_path = self
            .dirs
            .bucket_dir(bucket)
            .join(crate::keypath::key_to_relpath(key));

        // Streaming atomic write: temp → fsync → rename → row insert.
        let (temp_path, size, etag_hex) = self.stream_to_temp(body).await?;

        if let Some(parent) = final_path.parent() {
            if let Err(err) = tokio::fs::create_dir_all(parent).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(internal(err));
            }
        }
        if let Err(err) = tokio::fs::rename(&temp_path, &final_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(internal(err));
        }

        // Bytes are durably in place; write the authoritative row last.
        let row = ObjectRow {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            size,
            etag: etag_hex.clone(),
            // S3 defaults a missing content type to binary/octet-stream.
            content_type: content_type.or_else(|| Some(DEFAULT_CONTENT_TYPE.to_owned())),
            last_modified: unix_now(),
            metadata: metadata_json,
        };
        self.db_call(move |db| db.put_object(&row)).await?;
        Ok((etag_hex, size))
    }

    /// Create the bucket `name` if it does not already exist: make its directory
    /// (idempotent) and insert its row (idempotent — an existing bucket is left
    /// as-is, no error). Mirrors the CreateBucket ordering (directory first,
    /// then the row) so a crash between them reads as "does not exist". Used by
    /// the `--seed` loader for its declarative, re-runnable bucket creation.
    pub async fn create_bucket_if_missing(&self, name: &str) -> S3Result<()> {
        let dir = self.dirs.bucket_dir(name);
        tokio::fs::create_dir_all(&dir).await.map_err(internal)?;
        let name = name.to_owned();
        self.db_call(move |db| db.create_bucket(&name, unix_now()))
            .await?;
        Ok(())
    }

    /// Stream-copy a source object file into a fresh temp file in `.tmp/`,
    /// `fsync`, and return its path. Bytes flow through `tokio::io::copy` a
    /// chunk at a time — the object is never held in memory, the same discipline
    /// as [`Store::stream_to_temp`]. The ETag is not recomputed: CopyObject
    /// preserves the source row's ETag verbatim. The temp file is removed on
    /// error. Caller renames it into the destination key.
    async fn stage_file_copy(&self, src: &std::path::Path) -> S3Result<PathBuf> {
        use tokio::io::AsyncWriteExt;

        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = self
            .dirs
            .tmp_dir()
            .join(format!("{}-{n}.copy", std::process::id()));
        let mut out = tokio::fs::File::create(&temp_path)
            .await
            .map_err(internal)?;

        let copy_result: S3Result<()> = async {
            let mut input = tokio::fs::File::open(src).await.map_err(internal)?;
            tokio::io::copy(&mut input, &mut out)
                .await
                .map_err(internal)?;
            out.flush().await.map_err(internal)?;
            out.sync_all().await.map_err(internal)?; // durability before rename
            Ok(())
        }
        .await;

        if let Err(err) = copy_result {
            drop(out);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(err);
        }
        Ok(temp_path)
    }

    /// Filesystem path of one staged part: `.multipart/<upload_id>/<part_number>`.
    fn part_path(&self, upload_id: &str, part_number: i32) -> PathBuf {
        self.dirs
            .multipart_dir()
            .join(upload_id)
            .join(part_number.to_string())
    }

    /// Stream-concatenate the given parts (already in ascending order) into a
    /// fresh temp file in `.tmp/`, `fsync`, and return `(temp_path, total_size)`.
    /// Parts are copied through `tokio::io::copy` a chunk at a time — a whole
    /// part (or the assembled object) is never held in memory, the same
    /// discipline as [`Store::stream_to_temp`]. The temp file is removed on
    /// error.
    async fn assemble_parts(
        &self,
        upload_id: &str,
        parts: &[RecordedPart],
    ) -> S3Result<(PathBuf, i64)> {
        use tokio::io::AsyncWriteExt;

        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = self
            .dirs
            .tmp_dir()
            .join(format!("{}-{n}.assemble", std::process::id()));
        let mut out = tokio::fs::File::create(&temp_path)
            .await
            .map_err(internal)?;

        let mut total: i64 = 0;
        let copy_result: S3Result<()> = async {
            for part in parts {
                let src = self.part_path(upload_id, part.part_number);
                let mut input = tokio::fs::File::open(&src).await.map_err(internal)?;
                total += tokio::io::copy(&mut input, &mut out)
                    .await
                    .map_err(internal)? as i64;
            }
            out.flush().await.map_err(internal)?;
            out.sync_all().await.map_err(internal)?; // durability before rename
            Ok(())
        }
        .await;

        if let Err(err) = copy_result {
            drop(out);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(err);
        }
        Ok((temp_path, total))
    }

    /// Pick the right "not found" error when an object row is absent: if the
    /// bucket itself doesn't exist it's `NoSuchBucket`, otherwise `NoSuchKey`.
    /// Only called on the miss path, so the extra query is off the hot path.
    async fn missing_object_error(&self, bucket: &str) -> s3s::S3Error {
        let b = bucket.to_owned();
        match self.db_call(move |db| db.bucket_exists(&b)).await {
            Ok(true) => s3_error!(NoSuchKey),
            Ok(false) => s3_error!(NoSuchBucket),
            Err(err) => err,
        }
    }

    /// Run the pure listing engine against the SQLite seek primitive, entirely
    /// inside one `spawn_blocking` (the engine drives `fetch` synchronously and
    /// re-seeks for skip-scan). Shared by ListObjectsV2 and legacy ListObjects.
    async fn run_listing(
        &self,
        bucket: String,
        prefix: String,
        delimiter: Option<String>,
        start_from: Option<String>,
        skip_cp_le: Option<String>,
        max_keys: usize,
    ) -> S3Result<ListPage<ObjectRow>> {
        self.db_call(move |db| {
            // The engine calls `fetch` repeatedly; capture the first DB error and
            // surface it after the run rather than unwinding through the engine.
            let mut db_err: Option<rusqlite::Error> = None;
            let fetch = |from: Option<&str>, limit: i64| -> Vec<ObjectRow> {
                match db.list_objects_page(&bucket, &prefix, from, limit) {
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
                skip_cp_le: skip_cp_le.as_deref(),
                max_keys,
            };
            let page = listing::list_page(fetch, |r: &ObjectRow| r.key.as_str(), &params);
            match db_err {
                Some(e) => Err(e),
                None => Ok(page),
            }
        })
        .await
    }

    /// The fixed dev `Owner`, included only when the caller asks for it.
    fn owner(&self, include: bool) -> Option<Owner> {
        include.then(|| Owner {
            id: Some(self.access_key.clone()),
            display_name: Some(self.access_key.clone()),
        })
    }

    /// Run a blocking database closure off the async runtime, mapping both the
    /// join failure and any rusqlite error to an S3 `InternalError`.
    async fn db_call<T, F>(&self, f: F) -> S3Result<T>
    where
        F: FnOnce(Db) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || f(db))
            .await
            .map_err(internal)?
            .map_err(internal)
    }
}

/// Convert any error into an opaque S3 `InternalError`.
pub(crate) fn internal<E: std::fmt::Display>(e: E) -> s3s::S3Error {
    s3s::S3Error::with_message(s3s::S3ErrorCode::InternalError, e.to_string())
}

/// Current time as Unix seconds (saturating at the epoch).
pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse the stored JSON metadata object into an S3 `Metadata` map.
pub(crate) fn parse_metadata(json: &str) -> S3Result<Metadata> {
    serde_json::from_str(json).map_err(internal)
}

/// Apply S3's `max-keys` policy, which `s3s` passes through unvalidated:
/// default 1000, silently cap at 1000, `0` is a valid empty page, negative is
/// `400 InvalidArgument`.
fn resolve_max_keys(max_keys: Option<i32>) -> S3Result<usize> {
    match max_keys {
        None => Ok(1000),
        Some(n) if n < 0 => Err(s3_error!(InvalidArgument, "max-keys must be non-negative")),
        Some(n) => Ok((n as usize).min(1000)),
    }
}

/// Apply S3's `max-parts` policy for ListParts: default 1000, cap at 1000,
/// negative is `400 InvalidArgument`. Mirrors [`resolve_max_keys`].
fn resolve_max_parts(max_parts: Option<i32>) -> S3Result<usize> {
    match max_parts {
        None => Ok(1000),
        Some(n) if n < 0 => Err(s3_error!(InvalidArgument, "max-parts must be non-negative")),
        Some(n) => Ok((n as usize).min(1000)),
    }
}

/// Map a stored [`ObjectRow`] to a listing `Object`, with a fixed `STANDARD`
/// storage class and the ETag quoted the same way GET/PUT return it. `encode`
/// is presentation-only (identity, or [`listing::url_encode`] for
/// `encoding-type=url`) — the stored key is never mutated.
fn object_from_row(
    row: ObjectRow,
    owner: Option<Owner>,
    encode: &dyn Fn(&str) -> String,
) -> Object {
    Object {
        key: Some(encode(&row.key)),
        e_tag: Some(ETag::Strong(row.etag)),
        size: Some(row.size),
        last_modified: Some(ts_from_unix(row.last_modified)),
        storage_class: Some(ObjectStorageClass::from_static(
            ObjectStorageClass::STANDARD,
        )),
        owner,
        ..Default::default()
    }
}

/// The presentation encoder selected by a request's `encoding-type`: identity,
/// or percent-encoding when `url` was asked for. Applied to `Key`, `Prefix`,
/// `Delimiter`, `StartAfter`, and each `CommonPrefix`.
fn key_encoder(encoding_type: Option<&EncodingType>) -> Box<dyn Fn(&str) -> String> {
    if encoding_type.map(EncodingType::as_str) == Some(EncodingType::URL) {
        Box::new(|s: &str| listing::url_encode(s))
    } else {
        Box::new(|s: &str| s.to_owned())
    }
}

/// Build an S3 timestamp from Unix seconds.
pub(crate) fn ts_from_unix(secs: i64) -> Timestamp {
    let st = if secs >= 0 {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        SystemTime::UNIX_EPOCH - Duration::from_secs((-secs) as u64)
    };
    Timestamp::from(st)
}

#[async_trait::async_trait]
impl s3s::S3 for Store {
    async fn create_bucket(
        &self,
        req: S3Request<CreateBucketInput>,
    ) -> S3Result<S3Response<CreateBucketOutput>> {
        let bucket = req.input.bucket;
        // Any region / location constraint is accepted and ignored.

        // Directory first, then the row: a crash in between leaves an empty
        // orphan directory with no row, which reads as "does not exist".
        let dir = self.dirs.bucket_dir(&bucket);
        tokio::fs::create_dir_all(&dir).await.map_err(internal)?;

        let name = bucket.clone();
        let created = self
            .db_call(move |db| db.create_bucket(&name, unix_now()))
            .await?;
        if !created {
            return Err(s3_error!(
                BucketAlreadyOwnedByYou,
                "bucket already exists: {bucket}"
            ));
        }

        Ok(S3Response::new(CreateBucketOutput {
            location: Some(format!("/{bucket}")),
        }))
    }

    async fn put_object(
        &self,
        req: S3Request<PutObjectInput>,
    ) -> S3Result<S3Response<PutObjectOutput>> {
        let input = req.input;
        let bucket = input.bucket;
        let key = input.key;

        // The bucket must exist before we accept bytes.
        let b = bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&b)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {bucket}"));
        }

        // Delegate to the shared write path: temp → fsync → rename → row-last,
        // MD5 computed, content-type defaulted.
        let metadata =
            serde_json::to_string(&input.metadata.unwrap_or_default()).map_err(internal)?;
        let (etag_hex, size) = self
            .put_bytes(&bucket, &key, input.body, input.content_type, metadata)
            .await?;

        // Committed → fire an ObjectCreated:Put notification (async, best-effort).
        self.fire(ObjectEvent::created(
            &bucket,
            &key,
            EventKind::Put,
            size,
            &etag_hex,
        ));

        Ok(S3Response::new(PutObjectOutput {
            e_tag: Some(ETag::Strong(etag_hex)),
            ..Default::default()
        }))
    }

    async fn get_object(
        &self,
        req: S3Request<GetObjectInput>,
    ) -> S3Result<S3Response<GetObjectOutput>> {
        use std::io::SeekFrom;
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let bucket = req.input.bucket;
        let key = req.input.key;
        let range = req.input.range;

        let b = bucket.clone();
        let k = key.clone();
        let row = self.db_call(move |db| db.get_object(&b, &k)).await?;
        let Some(row) = row else {
            return Err(self.missing_object_error(&bucket).await);
        };
        let full_len = row.size as u64;

        // Resolve the byte range (if any) against the object length.
        let (offset, length, content_range, partial) = match range {
            Some(r) => {
                let span = r.check(full_len)?; // half-open; InvalidRange on failure
                let last = span.end.saturating_sub(1);
                let cr = format!("bytes {}-{}/{}", span.start, last, full_len);
                (span.start, span.end - span.start, Some(cr), true)
            }
            None => (0, full_len, None, false),
        };

        let path = self
            .dirs
            .bucket_dir(&bucket)
            .join(crate::keypath::key_to_relpath(&key));
        let mut file = tokio::fs::File::open(&path).await.map_err(internal)?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset)).await.map_err(internal)?;
        }
        let stream = tokio_util::io::ReaderStream::new(file.take(length));
        let body = StreamingBlob::wrap(stream);

        let output = GetObjectOutput {
            body: Some(body),
            content_length: Some(length as i64),
            content_range,
            content_type: row.content_type,
            e_tag: Some(ETag::Strong(row.etag)),
            last_modified: Some(ts_from_unix(row.last_modified)),
            metadata: Some(parse_metadata(&row.metadata)?),
            accept_ranges: Some("bytes".to_owned()),
            ..Default::default()
        };
        if partial {
            Ok(S3Response::with_status(
                output,
                hyper::StatusCode::PARTIAL_CONTENT,
            ))
        } else {
            Ok(S3Response::new(output))
        }
    }

    async fn delete_object(
        &self,
        req: S3Request<DeleteObjectInput>,
    ) -> S3Result<S3Response<DeleteObjectOutput>> {
        let bucket = req.input.bucket;
        let key = req.input.key;

        // Row first (source of truth), then unlink the bytes. Idempotent: a
        // missing key deletes zero rows and the unlink's NotFound is ignored,
        // matching S3's "delete always succeeds" contract.
        let b = bucket.clone();
        let k = key.clone();
        let removed = self.db_call(move |db| db.delete_object(&b, &k)).await?;

        let path = self
            .dirs
            .bucket_dir(&bucket)
            .join(crate::keypath::key_to_relpath(&key));
        if let Err(err) = tokio::fs::remove_file(&path).await {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(internal(err));
            }
        }

        // Fire only for a real removal (a never-existed key removes no row and
        // fires nothing — ObjectRemoved carries no size/eTag).
        if removed {
            self.fire(ObjectEvent::removed(&bucket, &key));
        }
        Ok(S3Response::new(DeleteObjectOutput::default()))
    }

    async fn head_object(
        &self,
        req: S3Request<HeadObjectInput>,
    ) -> S3Result<S3Response<HeadObjectOutput>> {
        let bucket = req.input.bucket;
        let key = req.input.key;
        let b = bucket.clone();
        let k = key.clone();
        let row = self.db_call(move |db| db.get_object(&b, &k)).await?;
        let Some(row) = row else {
            return Err(self.missing_object_error(&bucket).await);
        };
        Ok(S3Response::new(HeadObjectOutput {
            content_length: Some(row.size),
            e_tag: Some(ETag::Strong(row.etag)),
            content_type: row.content_type,
            last_modified: Some(ts_from_unix(row.last_modified)),
            metadata: Some(parse_metadata(&row.metadata)?),
            ..Default::default()
        }))
    }

    async fn head_bucket(
        &self,
        req: S3Request<HeadBucketInput>,
    ) -> S3Result<S3Response<HeadBucketOutput>> {
        let bucket = req.input.bucket;
        let exists = self.db_call(move |db| db.bucket_exists(&bucket)).await?;
        if !exists {
            return Err(s3_error!(NoSuchBucket));
        }
        Ok(S3Response::new(HeadBucketOutput::default()))
    }

    async fn delete_bucket(
        &self,
        req: S3Request<DeleteBucketInput>,
    ) -> S3Result<S3Response<DeleteBucketOutput>> {
        let bucket = req.input.bucket;
        let name = bucket.clone();
        let outcome = self.db_call(move |db| db.try_delete_bucket(&name)).await?;
        match outcome {
            DeleteBucketOutcome::Missing => return Err(s3_error!(NoSuchBucket)),
            DeleteBucketOutcome::NotEmpty => {
                return Err(s3_error!(BucketNotEmpty, "bucket is not empty: {bucket}"))
            }
            DeleteBucketOutcome::Deleted => {}
        }
        // Row gone (source of truth); now remove the directory tree, including
        // any empty prefix subdirectories left by deleted objects.
        let dir = self.dirs.bucket_dir(&bucket);
        if let Err(err) = tokio::fs::remove_dir_all(&dir).await {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(internal(err));
            }
        }
        Ok(S3Response::new(DeleteBucketOutput::default()))
    }

    async fn list_buckets(
        &self,
        _req: S3Request<ListBucketsInput>,
    ) -> S3Result<S3Response<ListBucketsOutput>> {
        let rows = self.db_call(|db| db.list_buckets()).await?;
        let buckets: Vec<Bucket> = rows
            .into_iter()
            .map(|row| Bucket {
                name: Some(row.name),
                creation_date: Some(ts_from_unix(row.created_at)),
                ..Default::default()
            })
            .collect();
        Ok(S3Response::new(ListBucketsOutput {
            buckets: Some(buckets),
            ..Default::default()
        }))
    }

    async fn list_objects_v2(
        &self,
        req: S3Request<ListObjectsV2Input>,
    ) -> S3Result<S3Response<ListObjectsV2Output>> {
        let input = req.input;
        let bucket = input.bucket;
        let prefix = input.prefix.unwrap_or_default();
        let max_keys = resolve_max_keys(input.max_keys)?;

        // A listing on a bucket that doesn't exist is NoSuchBucket, not empty.
        let b = bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&b)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {bucket}"));
        }

        // Cursor precedence: an opaque continuation-token wins over start-after
        // (which is only honored on the first page).
        let start_from = match (&input.continuation_token, &input.start_after) {
            (Some(tok), _) => Some(
                listing::decode_token(tok)
                    .map_err(|_| s3_error!(InvalidArgument, "invalid continuation-token"))?,
            ),
            (None, Some(sa)) => Some(format!("{sa}\0")),
            (None, None) => None,
        };

        let page = self
            .run_listing(
                bucket.clone(),
                prefix.clone(),
                input.delimiter.clone(),
                start_from,
                None, // v2 has no marker skip
                max_keys,
            )
            .await?;

        // `encoding-type=url` percent-encodes presentation of keys/prefixes/
        // delimiter/start-after so XML-unsafe bytes round-trip; stored keys are
        // untouched.
        let encode = key_encoder(input.encoding_type.as_ref());

        let owner = self.owner(input.fetch_owner.unwrap_or(false));
        let key_count = (page.contents.len() + page.common_prefixes.len()) as i32;
        let contents: Vec<Object> = page
            .contents
            .into_iter()
            .map(|row| object_from_row(row, owner.clone(), encode.as_ref()))
            .collect();
        let common_prefixes: Vec<CommonPrefix> = page
            .common_prefixes
            .into_iter()
            .map(|p| CommonPrefix {
                prefix: Some(encode(&p)),
            })
            .collect();
        let next_continuation_token = page.next_cursor.as_deref().map(listing::encode_token);

        Ok(S3Response::new(ListObjectsV2Output {
            name: Some(bucket),
            prefix: Some(encode(&prefix)),
            delimiter: input.delimiter.as_deref().map(&encode),
            max_keys: Some(max_keys as i32),
            key_count: Some(key_count),
            is_truncated: Some(page.is_truncated),
            contents: (!contents.is_empty()).then_some(contents),
            common_prefixes: (!common_prefixes.is_empty()).then_some(common_prefixes),
            continuation_token: input.continuation_token,
            next_continuation_token,
            start_after: input.start_after.as_deref().map(&encode),
            encoding_type: input.encoding_type,
            ..Default::default()
        }))
    }

    async fn list_objects(
        &self,
        req: S3Request<ListObjectsInput>,
    ) -> S3Result<S3Response<ListObjectsOutput>> {
        let input = req.input;
        let bucket = input.bucket;
        let prefix = input.prefix.unwrap_or_default();
        let max_keys = resolve_max_keys(input.max_keys)?;

        let b = bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&b)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {bucket}"));
        }

        // v1 uses a plaintext `marker` = "start strictly after this key". A key
        // resumes at `marker\0`; groups already covered (`CommonPrefix <=
        // marker`) are skipped so a delimiter-resume doesn't re-emit them.
        let (start_from, skip_cp_le) = match &input.marker {
            Some(m) => (Some(format!("{m}\0")), Some(m.clone())),
            None => (None, None),
        };

        let page = self
            .run_listing(
                bucket.clone(),
                prefix.clone(),
                input.delimiter.clone(),
                start_from,
                skip_cp_le,
                max_keys,
            )
            .await?;

        let encode = key_encoder(input.encoding_type.as_ref());
        // v1 always carries Owner (there is no fetch-owner toggle).
        let owner = self.owner(true);
        let contents: Vec<Object> = page
            .contents
            .into_iter()
            .map(|row| object_from_row(row, owner.clone(), encode.as_ref()))
            .collect();
        let common_prefixes: Vec<CommonPrefix> = page
            .common_prefixes
            .into_iter()
            .map(|p| CommonPrefix {
                prefix: Some(encode(&p)),
            })
            .collect();
        // S3 quirk: NextMarker is present only when a delimiter is set (and the
        // page is truncated). Without a delimiter the client resumes from the
        // last `Key` itself.
        let next_marker = match (input.delimiter.is_some(), page.next_marker) {
            (true, Some(m)) => Some(encode(&m)),
            _ => None,
        };

        Ok(S3Response::new(ListObjectsOutput {
            name: Some(bucket),
            prefix: Some(encode(&prefix)),
            marker: input.marker.as_deref().map(&encode),
            delimiter: input.delimiter.as_deref().map(&encode),
            max_keys: Some(max_keys as i32),
            is_truncated: Some(page.is_truncated),
            contents: (!contents.is_empty()).then_some(contents),
            common_prefixes: (!common_prefixes.is_empty()).then_some(common_prefixes),
            next_marker,
            encoding_type: input.encoding_type,
            ..Default::default()
        }))
    }

    async fn create_multipart_upload(
        &self,
        req: S3Request<CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<CreateMultipartUploadOutput>> {
        let input = req.input;
        let bucket = input.bucket;
        let key = input.key;

        // The bucket must exist before we open an upload against it.
        let b = bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&b)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {bucket}"));
        }

        let upload_id = multipart::new_upload_id();

        // Staging dir first, then the row: a crash in between leaves an empty
        // orphan dir with no row — invisible, sweepable like `.tmp/`.
        let stage = self.dirs.multipart_dir().join(&upload_id);
        tokio::fs::create_dir_all(&stage).await.map_err(internal)?;

        let metadata =
            serde_json::to_string(&input.metadata.unwrap_or_default()).map_err(internal)?;
        let row = MultipartRow {
            upload_id: upload_id.clone(),
            bucket: bucket.clone(),
            key: key.clone(),
            content_type: input.content_type,
            metadata,
            started_at: unix_now(),
        };
        self.db_call(move |db| db.create_multipart(&row)).await?;

        Ok(S3Response::new(CreateMultipartUploadOutput {
            bucket: Some(bucket),
            key: Some(key),
            upload_id: Some(upload_id),
            ..Default::default()
        }))
    }

    async fn upload_part(
        &self,
        req: S3Request<UploadPartInput>,
    ) -> S3Result<S3Response<UploadPartOutput>> {
        let input = req.input;
        let upload_id = input.upload_id;
        let part_number = input.part_number;

        // S3's accepted part-number range.
        if !(1..=10000).contains(&part_number) {
            return Err(s3_error!(
                InvalidArgument,
                "part number must be in 1..=10000, got {part_number}"
            ));
        }

        // The upload must exist (a bogus/expired id is NoSuchUpload).
        let uid = upload_id.clone();
        if self
            .db_call(move |db| db.get_multipart(&uid))
            .await?
            .is_none()
        {
            return Err(s3_error!(NoSuchUpload, "no such upload: {upload_id}"));
        }

        // Same streaming/fsync discipline as PutObject: land in `.tmp/`, then
        // rename atomically into the part's staging slot.
        let (temp_path, size, etag_hex) = self.stream_to_temp(input.body).await?;
        let final_path = self.part_path(&upload_id, part_number);
        if let Some(parent) = final_path.parent() {
            if let Err(err) = tokio::fs::create_dir_all(parent).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(internal(err));
            }
        }
        if let Err(err) = tokio::fs::rename(&temp_path, &final_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(internal(err));
        }

        // Bytes durably in place; record the part (INSERT OR REPLACE = last
        // write wins on re-upload).
        let etag = etag_hex.clone();
        self.db_call(move |db| db.put_part(&upload_id, part_number, size, &etag))
            .await?;

        Ok(S3Response::new(UploadPartOutput {
            e_tag: Some(ETag::Strong(etag_hex)),
            ..Default::default()
        }))
    }

    async fn list_parts(
        &self,
        req: S3Request<ListPartsInput>,
    ) -> S3Result<S3Response<ListPartsOutput>> {
        let input = req.input;
        let bucket = input.bucket;
        let key = input.key;
        let upload_id = input.upload_id;
        let max_parts = resolve_max_parts(input.max_parts)?;
        let marker = input.part_number_marker;

        let uid = upload_id.clone();
        if self
            .db_call(move |db| db.get_multipart(&uid))
            .await?
            .is_none()
        {
            return Err(s3_error!(NoSuchUpload, "no such upload: {upload_id}"));
        }

        // Fetch one extra row to detect truncation without a second query.
        let uid = upload_id.clone();
        let limit = max_parts as i64 + 1;
        let mut rows = self
            .db_call(move |db| db.list_parts(&uid, marker, limit))
            .await?;
        let is_truncated = rows.len() > max_parts;
        rows.truncate(max_parts);
        let next_marker = is_truncated
            .then(|| rows.last().map(|p| p.part_number))
            .flatten();

        let started_at = ts_from_unix(unix_now());
        let parts: Vec<Part> = rows
            .into_iter()
            .map(|p| Part {
                part_number: Some(p.part_number),
                size: Some(p.size),
                e_tag: Some(ETag::Strong(p.etag)),
                last_modified: Some(started_at.clone()),
                ..Default::default()
            })
            .collect();

        Ok(S3Response::new(ListPartsOutput {
            bucket: Some(bucket),
            key: Some(key),
            upload_id: Some(upload_id),
            storage_class: Some(StorageClass::from_static(StorageClass::STANDARD)),
            max_parts: Some(max_parts as i32),
            part_number_marker: marker,
            next_part_number_marker: next_marker,
            is_truncated: Some(is_truncated),
            parts: (!parts.is_empty()).then_some(parts),
            ..Default::default()
        }))
    }

    async fn complete_multipart_upload(
        &self,
        req: S3Request<CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<CompleteMultipartUploadOutput>> {
        let input = req.input;
        let bucket = input.bucket;
        let key = input.key;
        let upload_id = input.upload_id;

        // Resolve the upload (unknown id → NoSuchUpload).
        let uid = upload_id.clone();
        let Some(upload) = self.db_call(move |db| db.get_multipart(&uid)).await? else {
            return Err(s3_error!(NoSuchUpload, "no such upload: {upload_id}"));
        };

        // The client's submitted part list, in the order given.
        let submitted: Vec<SubmittedPart> = input
            .multipart_upload
            .and_then(|m| m.parts)
            .unwrap_or_default()
            .into_iter()
            .map(|p| SubmittedPart {
                part_number: p.part_number.unwrap_or(0),
                etag: p.e_tag.map(|e| e.into_value()).unwrap_or_default(),
            })
            .collect();

        // All recorded parts, then validate the submission (all before assembly).
        let uid = upload_id.clone();
        let recorded_rows = self.db_call(move |db| db.all_parts(&uid)).await?;
        let recorded: Vec<RecordedPart> = recorded_rows
            .into_iter()
            .map(|p: PartRow| RecordedPart {
                part_number: p.part_number,
                size: p.size,
                etag_hex: p.etag,
            })
            .collect();

        let selected =
            multipart::validate_complete(&submitted, &recorded).map_err(|e| match e {
                CompleteError::Empty => s3_error!(InvalidRequest, "part list must not be empty"),
                CompleteError::OutOfOrder => {
                    s3_error!(InvalidPartOrder, "parts must be in ascending order")
                }
                CompleteError::InvalidPart { part_number } => {
                    s3_error!(InvalidPart, "invalid or missing part {part_number}")
                }
            })?;

        // Composite ETag from the recorded hex digests — no data re-read.
        let hexes: Vec<&str> = selected.iter().map(|p| p.etag_hex.as_str()).collect();
        let composite = multipart::composite_etag(&hexes);

        // Stream-assemble the parts into `.tmp/`, then atomically rename into
        // `buckets/<b>/<key>` (parents created). A crash between rename and the
        // row insert leaves a harmless orphan file, exactly like PutObject.
        let (temp_path, total_size) = self.assemble_parts(&upload_id, &selected).await?;
        let final_path = self
            .dirs
            .bucket_dir(&bucket)
            .join(crate::keypath::key_to_relpath(&key));
        if let Some(parent) = final_path.parent() {
            if let Err(err) = tokio::fs::create_dir_all(parent).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(internal(err));
            }
        }
        if let Err(err) = tokio::fs::rename(&temp_path, &final_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(internal(err));
        }

        // Write the object row and drop the multipart bookkeeping in one txn.
        let object = ObjectRow {
            bucket: bucket.clone(),
            key: key.clone(),
            size: total_size,
            etag: composite.clone(),
            content_type: upload
                .content_type
                .or_else(|| Some(DEFAULT_CONTENT_TYPE.to_owned())),
            last_modified: unix_now(),
            metadata: upload.metadata,
        };
        let uid = upload_id.clone();
        self.db_call(move |db| db.complete_multipart(&uid, &object))
            .await?;

        // Committed → one ObjectCreated:CompleteMultipartUpload (not one per
        // part), carrying the `-N` composite ETag.
        self.fire(ObjectEvent::created(
            &bucket,
            &key,
            EventKind::CompleteMultipartUpload,
            total_size,
            &composite,
        ));

        // Staging tree no longer referenced; remove it (best-effort — an orphan
        // dir is invisible and sweepable).
        let stage = self.dirs.multipart_dir().join(&upload_id);
        let _ = tokio::fs::remove_dir_all(&stage).await;

        Ok(S3Response::new(CompleteMultipartUploadOutput {
            bucket: Some(bucket.clone()),
            key: Some(key.clone()),
            e_tag: Some(ETag::Strong(composite)),
            location: Some(format!("/{bucket}/{key}")),
            ..Default::default()
        }))
    }

    async fn abort_multipart_upload(
        &self,
        req: S3Request<AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<AbortMultipartUploadOutput>> {
        let upload_id = req.input.upload_id;

        // A live id → success; an already-gone one → NoSuchUpload.
        let uid = upload_id.clone();
        if self
            .db_call(move |db| db.get_multipart(&uid))
            .await?
            .is_none()
        {
            return Err(s3_error!(NoSuchUpload, "no such upload: {upload_id}"));
        }

        // Rows first (source of truth), then the staging tree.
        let uid = upload_id.clone();
        self.db_call(move |db| db.delete_multipart(&uid)).await?;
        let stage = self.dirs.multipart_dir().join(&upload_id);
        if let Err(err) = tokio::fs::remove_dir_all(&stage).await {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(internal(err));
            }
        }

        Ok(S3Response::new(AbortMultipartUploadOutput::default()))
    }

    async fn copy_object(
        &self,
        req: S3Request<CopyObjectInput>,
    ) -> S3Result<S3Response<CopyObjectOutput>> {
        let input = req.input;
        let dest_bucket = input.bucket;
        let dest_key = input.key;

        // Only the `<bucket>/<key>` copy-source form is supported; access-point
        // and Outpost ARNs never reach a path-style dev tool.
        let (src_bucket, src_key) = match input.copy_source {
            CopySource::Bucket { bucket, key, .. } => (bucket.to_string(), key.to_string()),
            _ => {
                return Err(s3_error!(
                    NotImplemented,
                    "only <bucket>/<key> copy-source is supported"
                ))
            }
        };

        // Resolution errors, all before any bytes move. Loading the source row
        // distinguishes NoSuchBucket (source bucket gone) from NoSuchKey.
        let sb = src_bucket.clone();
        let sk = src_key.clone();
        let Some(src_row) = self.db_call(move |db| db.get_object(&sb, &sk)).await? else {
            return Err(self.missing_object_error(&src_bucket).await);
        };
        let db_name = dest_bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&db_name)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {dest_bucket}"));
        }

        let replace = input
            .metadata_directive
            .as_ref()
            .map(MetadataDirective::as_str)
            == Some(MetadataDirective::REPLACE);
        let same_object = src_bucket == dest_bucket && src_key == dest_key;

        // Copying an object onto itself without changing metadata is illegal in
        // S3 (SDKs probe this); only a REPLACE makes a self-copy meaningful.
        if same_object && !replace {
            return Err(s3_error!(
                InvalidRequest,
                "This copy request is illegal because it is trying to copy an object to \
                 itself without changing the object's metadata, storage class, website \
                 redirect location or encryption attributes."
            ));
        }

        // COPY (default) carries the source row's content-type + user metadata;
        // REPLACE takes them from the request (content-type defaulting as PUT).
        let (content_type, metadata) = if replace {
            let ct = input
                .content_type
                .or_else(|| Some(DEFAULT_CONTENT_TYPE.to_owned()));
            let md =
                serde_json::to_string(&input.metadata.unwrap_or_default()).map_err(internal)?;
            (ct, md)
        } else {
            (src_row.content_type.clone(), src_row.metadata.clone())
        };

        // Bytes: for a self-copy the file already holds the right bytes, so skip
        // the copy (metadata-only update). Otherwise stage the source through
        // `.tmp/` → fsync → atomic rename into the destination key, reusing the
        // Phase 1 write discipline so the copy is one real assembled file.
        if !same_object {
            let src_path = self
                .dirs
                .bucket_dir(&src_bucket)
                .join(crate::keypath::key_to_relpath(&src_key));
            let final_path = self
                .dirs
                .bucket_dir(&dest_bucket)
                .join(crate::keypath::key_to_relpath(&dest_key));
            let temp_path = self.stage_file_copy(&src_path).await?;
            if let Some(parent) = final_path.parent() {
                if let Err(err) = tokio::fs::create_dir_all(parent).await {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err(internal(err));
                }
            }
            if let Err(err) = tokio::fs::rename(&temp_path, &final_path).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(internal(err));
            }
        }

        // Bytes durably in place; write the authoritative row last. The ETag is
        // the source's, preserved verbatim (a multipart source keeps its `-N`).
        let now = unix_now();
        let etag = src_row.etag.clone();
        let size = src_row.size;
        let row = ObjectRow {
            bucket: dest_bucket.clone(),
            key: dest_key.clone(),
            size,
            etag: etag.clone(),
            content_type,
            last_modified: now,
            metadata,
        };
        self.db_call(move |db| db.put_object(&row)).await?;

        // Committed → ObjectCreated:Copy for the destination key.
        self.fire(ObjectEvent::created(
            &dest_bucket,
            &dest_key,
            EventKind::Copy,
            size,
            &etag,
        ));

        Ok(S3Response::new(CopyObjectOutput {
            copy_object_result: Some(CopyObjectResult {
                e_tag: Some(ETag::Strong(etag)),
                last_modified: Some(ts_from_unix(now)),
                ..Default::default()
            }),
            ..Default::default()
        }))
    }

    async fn delete_objects(
        &self,
        req: S3Request<DeleteObjectsInput>,
    ) -> S3Result<S3Response<DeleteObjectsOutput>> {
        let input = req.input;
        let bucket = input.bucket;
        let quiet = input.delete.quiet.unwrap_or(false);
        // `versionId` on an entry is ignored (no versioning); the key is deleted.
        let keys: Vec<String> = input.delete.objects.into_iter().map(|o| o.key).collect();

        // S3 bounds a batch at 1000 keys; `s3s` doesn't enforce it, so we do.
        if keys.len() > 1000 {
            return Err(s3_error!(
                InvalidRequest,
                "the batch delete request must contain 1000 keys or fewer"
            ));
        }

        // A batch against a nonexistent bucket fails wholesale (no per-key
        // results), matching S3.
        let b = bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&b)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {bucket}"));
        }

        // Rows first (source of truth) in one transaction, then best-effort
        // unlink each file — the Phase 1 delete ordering, a NotFound ignored.
        let b = bucket.clone();
        let ks = keys.clone();
        let removed = self.db_call(move |db| db.delete_objects(&b, &ks)).await?;
        for key in &keys {
            let path = self
                .dirs
                .bucket_dir(&bucket)
                .join(crate::keypath::key_to_relpath(key));
            if let Err(err) = tokio::fs::remove_file(&path).await {
                if err.kind() != std::io::ErrorKind::NotFound {
                    return Err(internal(err));
                }
            }
        }

        // One ObjectRemoved:Delete per key that actually existed (a never-existed
        // key in the batch fires nothing).
        for key in &removed {
            self.fire(ObjectEvent::removed(&bucket, key));
        }

        // Non-quiet: a `Deleted` entry per requested key (S3 reports even a
        // never-existed key as deleted). Quiet: only errors, of which a
        // dev-tool happy path has none — so an empty `DeleteResult`.
        let deleted = (!quiet).then(|| {
            keys.into_iter()
                .map(|key| DeletedObject {
                    key: Some(key),
                    ..Default::default()
                })
                .collect()
        });

        Ok(S3Response::new(DeleteObjectsOutput {
            deleted,
            ..Default::default()
        }))
    }

    async fn put_bucket_cors(
        &self,
        req: S3Request<PutBucketCorsInput>,
    ) -> S3Result<S3Response<PutBucketCorsOutput>> {
        let input = req.input;
        let bucket = input.bucket;

        // CORS is bucket state, so the bucket must exist.
        let b = bucket.clone();
        if !self.db_call(move |db| db.bucket_exists(&b)).await? {
            return Err(s3_error!(NoSuchBucket, "no such bucket: {bucket}"));
        }

        // `s3s` parsed the XML; convert the DTO rules to our domain type, then
        // validate (≥1 rule; each with ≥1 origin, ≥1 method; methods in the S3
        // set) before storing. A bad config is rejected and persists nothing.
        let rules: Vec<crate::cors::CorsRule> = input
            .cors_configuration
            .cors_rules
            .into_iter()
            .map(domain_rule_from_dto)
            .collect();
        if let Err(msg) = crate::cors::validate(&rules) {
            return Err(s3_error!(InvalidRequest, "{msg}"));
        }
        let json = serde_json::to_string(&rules).map_err(internal)?;

        // Whole-config replace (INSERT OR REPLACE) — AWS `PutBucketCors` semantics.
        let b = bucket.clone();
        self.db_call(move |db| db.put_bucket_cors(&b, &json))
            .await?;

        Ok(S3Response::new(PutBucketCorsOutput::default()))
    }

    async fn get_bucket_cors(
        &self,
        req: S3Request<GetBucketCorsInput>,
    ) -> S3Result<S3Response<GetBucketCorsOutput>> {
        let bucket = req.input.bucket;

        let b = bucket.clone();
        let stored = self.db_call(move |db| db.get_bucket_cors(&b)).await?;
        let Some(json) = stored else {
            // The exact S3 error an SDK's "does this bucket have CORS?" probe
            // expects when a bucket has none.
            return Err(s3_error!(
                NoSuchCORSConfiguration,
                "no CORS configuration for bucket: {bucket}"
            ));
        };
        let rules = crate::cors::parse_rules(&json).map_err(internal)?;
        let cors_rules = rules.into_iter().map(dto_rule_from_domain).collect();

        Ok(S3Response::new(GetBucketCorsOutput {
            cors_rules: Some(cors_rules),
        }))
    }

    async fn delete_bucket_cors(
        &self,
        req: S3Request<DeleteBucketCorsInput>,
    ) -> S3Result<S3Response<DeleteBucketCorsOutput>> {
        let bucket = req.input.bucket;
        // Idempotent — deleting when none exists is not an error (AWS's tolerant
        // 204). No bucket-existence check: a delete on a gone bucket is a no-op.
        self.db_call(move |db| db.delete_bucket_cors(&bucket))
            .await?;
        Ok(S3Response::new(DeleteBucketCorsOutput::default()))
    }
}

/// Convert an `s3s` `CORSRule` DTO (parsed from the XML) into our domain rule,
/// flattening the `Option` list fields to empty vecs.
fn domain_rule_from_dto(r: CORSRule) -> crate::cors::CorsRule {
    crate::cors::CorsRule {
        allowed_origins: r.allowed_origins,
        allowed_methods: r.allowed_methods,
        allowed_headers: r.allowed_headers.unwrap_or_default(),
        expose_headers: r.expose_headers.unwrap_or_default(),
        max_age_seconds: r.max_age_seconds,
        id: r.id,
    }
}

/// Convert a domain rule back into an `s3s` `CORSRule` DTO for `GetBucketCors`,
/// re-wrapping empty list fields as `None` so the serialized XML omits them.
fn dto_rule_from_domain(r: crate::cors::CorsRule) -> CORSRule {
    CORSRule {
        allowed_headers: (!r.allowed_headers.is_empty()).then_some(r.allowed_headers),
        allowed_methods: r.allowed_methods,
        allowed_origins: r.allowed_origins,
        expose_headers: (!r.expose_headers.is_empty()).then_some(r.expose_headers),
        id: r.id,
        max_age_seconds: r.max_age_seconds,
    }
}
