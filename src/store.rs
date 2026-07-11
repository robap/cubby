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
    Bucket, CreateBucketInput, CreateBucketOutput, DeleteBucketInput, DeleteBucketOutput,
    DeleteObjectInput, DeleteObjectOutput, ETag, GetObjectInput, GetObjectOutput, HeadBucketInput,
    HeadBucketOutput, HeadObjectInput, HeadObjectOutput, ListBucketsInput, ListBucketsOutput,
    Metadata, PutObjectInput, PutObjectOutput, StreamingBlob, Timestamp,
};
use s3s::{s3_error, S3Request, S3Response, S3Result};

use crate::datadir::DataDir;
use crate::db::{Db, DeleteBucketOutcome, ObjectRow};

/// Monotonic counter for unique temp-file names within this process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// S3's default content type when a client supplies none.
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

/// The filesystem + SQLite object store.
#[derive(Clone)]
pub struct Store {
    db: Db,
    dirs: DataDir,
}

impl Store {
    pub fn new(db: Db, dirs: DataDir) -> Self {
        Self { db, dirs }
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

        let final_path = self
            .dirs
            .bucket_dir(&bucket)
            .join(crate::keypath::key_to_relpath(&key));

        // Streaming atomic write: temp → fsync → rename → row insert.
        let (temp_path, size, etag_hex) = self.stream_to_temp(input.body).await?;

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
        let metadata =
            serde_json::to_string(&input.metadata.unwrap_or_default()).map_err(internal)?;
        let row = ObjectRow {
            bucket,
            key,
            size,
            etag: etag_hex.clone(),
            // S3 defaults a missing content type to binary/octet-stream.
            content_type: input
                .content_type
                .or_else(|| Some(DEFAULT_CONTENT_TYPE.to_owned())),
            last_modified: unix_now(),
            metadata,
        };
        self.db_call(move |db| db.put_object(&row)).await?;

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
        self.db_call(move |db| db.delete_object(&b, &k)).await?;

        let path = self
            .dirs
            .bucket_dir(&bucket)
            .join(crate::keypath::key_to_relpath(&key));
        if let Err(err) = tokio::fs::remove_file(&path).await {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(internal(err));
            }
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
}
