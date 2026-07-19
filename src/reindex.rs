//! `cubby reindex <dir>`: scan `buckets/` and backfill `meta.sqlite` so
//! hand-dropped files become first-class objects — the inverse of cubby's usual
//! SQLite-drives-filesystem flow, and what *the filesystem is the API too*
//! implies. It is a **synchronous, offline** batch (`std::fs` walk + streamed
//! MD5; no tokio runtime, no port, no `Store`, no `Notifier`) that reads
//! pre-existing bytes instead of writing new ones.
//!
//! The engine is additive and non-destructive: it only *inserts* rows for files
//! with no row, leaving already-indexed rows (and their real content-type / user
//! metadata / multipart `-N` ETags) untouched — so re-runs are cheap and
//! idempotent. It touches only [`Db`] + [`DataDir`], so it can never fire
//! webhooks or serve traffic.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Context;

use crate::datadir::DataDir;
use crate::db::{Db, ObjectRow};
use crate::keypath::relpath_to_key;

/// S3's default content type when the extension guess comes up empty — the same
/// fallback PutObject/seed apply for a client that supplies none.
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

/// Counts from one [`run`], printed as the command's summary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReindexReport {
    /// Bucket directories that had no `buckets` row and were created.
    pub buckets_adopted: usize,
    /// Bucket directories that already had a row (left as-is).
    pub buckets_present: usize,
    /// Files with no `objects` row, for which a row was inserted.
    pub objects_indexed: usize,
    /// Files that already had a row (skipped, non-destructively).
    pub objects_present: usize,
    /// Entries skipped without indexing: symlinks (not followed), and loose
    /// files sitting directly under `buckets/` (belonging to no bucket).
    pub objects_skipped: usize,
}

impl std::fmt::Display for ReindexReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "  buckets: {} adopted, {} already present",
            self.buckets_adopted, self.buckets_present
        )?;
        write!(
            f,
            "  objects: {} indexed, {} already present, {} skipped",
            self.objects_indexed, self.objects_present, self.objects_skipped
        )
    }
}

/// Scan `dirs.buckets_dir()` and backfill `db` with a row for every regular file
/// that has none. Returns the tallies.
///
/// The walk is confined to `buckets/<name>/…`: cubby's own siblings
/// (`meta.sqlite`, `.gitignore`, `.tmp/`, `.multipart/`) live *outside*
/// `buckets/` and are never visited. Each directory directly under `buckets/` is
/// a bucket (adopted if it has no row); each regular file at any depth beneath a
/// bucket dir is a candidate object. Symlinks are never followed, and a loose
/// file sitting directly under `buckets/` (belonging to no bucket) is skipped —
/// both counted, never indexed.
pub fn run(dirs: &DataDir, db: &Db) -> anyhow::Result<ReindexReport> {
    let mut report = ReindexReport::default();
    let buckets_dir = dirs.buckets_dir();

    let entries = match fs::read_dir(&buckets_dir) {
        Ok(entries) => entries,
        // A dir with no `buckets/` yet (shouldn't happen post-bootstrap) has
        // nothing to adopt.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(e) => {
            return Err(e).with_context(|| format!("reading {}", buckets_dir.display()));
        }
    };

    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", buckets_dir.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("stat-ing {}", entry.path().display()))?;

        // Symlinks are not followed (avoids escaping the tree); a non-directory
        // entry directly under `buckets/` belongs to no bucket. Both are skipped.
        if file_type.is_symlink() || !file_type.is_dir() {
            report.objects_skipped += 1;
            continue;
        }

        // A bucket directory: adopt its row if missing (the name is the bucket
        // name verbatim — bucket names are not percent-encoded on disk).
        let bucket = entry.file_name().to_string_lossy().into_owned();
        if db.bucket_exists(&bucket)? {
            report.buckets_present += 1;
        } else {
            let created_at = mtime_secs(&entry.path());
            db.create_bucket(&bucket, created_at)?;
            report.buckets_adopted += 1;
        }

        let bucket_root = entry.path();
        index_dir(db, &bucket, &bucket_root, &bucket_root, &mut report)?;
    }

    Ok(report)
}

/// Recursively index one bucket subtree. `bucket_root` is the bucket directory
/// (fixed across the recursion); `dir` is the directory currently being read.
/// Each regular file's key is recovered from its `bucket_root`-relative path.
fn index_dir(
    db: &Db,
    bucket: &str,
    bucket_root: &Path,
    dir: &Path,
    report: &mut ReindexReport,
) -> anyhow::Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("stat-ing {}", path.display()))?;

        if file_type.is_symlink() {
            // Not followed — a symlink can point outside the tree.
            report.objects_skipped += 1;
            continue;
        }
        if file_type.is_dir() {
            index_dir(db, bucket, bucket_root, &path, report)?;
            continue;
        }
        if !file_type.is_file() {
            // FIFOs, sockets, devices — not objects.
            report.objects_skipped += 1;
            continue;
        }

        // Recover the canonical key from the bucket-relative path.
        let rel = path
            .strip_prefix(bucket_root)
            .expect("walked path is under the bucket root");
        let key = relpath_to_key(rel);

        // Additive & non-destructive: a file that already has a row is left
        // untouched (its real content-type / metadata / multipart ETag survive).
        if db.get_object(bucket, &key)?.is_some() {
            report.objects_present += 1;
            continue;
        }

        let (size, etag) =
            hash_file(&path).with_context(|| format!("hashing {}", path.display()))?;
        // Guess the content type from the recovered key's final segment (its
        // filename), defaulting to octet-stream like a client PUT with none.
        let filename = key.rsplit('/').next().unwrap_or(&key);
        let content_type = guess_content_type(filename).unwrap_or(DEFAULT_CONTENT_TYPE);
        db.put_object(&ObjectRow {
            bucket: bucket.to_owned(),
            key,
            size,
            etag,
            content_type: Some(content_type.to_owned()),
            last_modified: mtime_secs(&path),
            // User `x-amz-meta-*` metadata is not stored on disk; there is
            // nothing to recover.
            metadata: "{}".to_owned(),
        })?;
        report.objects_indexed += 1;
    }
    Ok(())
}

/// Stream a file through MD5, never buffering the whole thing, and return
/// `(size, hex content-MD5)` — the single-part ETag a plain `PutObject` of these
/// bytes would produce.
fn hash_file(path: &Path) -> std::io::Result<(i64, String)> {
    use md5::{Digest, Md5};

    let mut file = fs::File::open(path)?;
    let mut hasher = Md5::new();
    let mut buf = [0u8; 64 * 1024];
    let mut size: i64 = 0;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as i64;
    }
    Ok((size, hex::encode(hasher.finalize())))
}

/// A path's modification time as Unix seconds, saturating at the epoch for
/// pre-1970 or unreadable mtimes. Reindex records the file's real mtime, not the
/// original PUT time (which is not recoverable).
fn mtime_secs(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Guess a content type from a filename's extension via a small hand-rolled
/// table of dev-common types (no new dependency). Returns `None` for an unknown
/// or missing extension — the caller then defaults to `application/octet-stream`,
/// exactly as PutObject/seed do for a client that supplies no content type.
///
/// The extension is matched case-insensitively (`Notes.TXT` → `text/plain`).
pub fn guess_content_type(filename: &str) -> Option<&'static str> {
    // The extension is everything after the final `.`; a name with no `.` (or a
    // leading-dot dotfile like `.gitignore`, whose only `.` is at index 0) has
    // no extension to guess from.
    let ext = match filename.rfind('.') {
        Some(pos) if pos > 0 => &filename[pos + 1..],
        _ => return None,
    };
    let ext = ext.to_ascii_lowercase();
    let ct = match ext.as_str() {
        "txt" | "text" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "wasm" => "application/wasm",
        _ => return None,
    };
    Some(ct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_known_extensions_case_insensitively() {
        assert_eq!(guess_content_type("notes.txt"), Some("text/plain"));
        assert_eq!(guess_content_type("data.json"), Some("application/json"));
        assert_eq!(guess_content_type("page.html"), Some("text/html"));
        assert_eq!(guess_content_type("logo.png"), Some("image/png"));
        assert_eq!(guess_content_type("cat.JPG"), Some("image/jpeg"));
        assert_eq!(guess_content_type("photo.jpeg"), Some("image/jpeg"));
        assert_eq!(guess_content_type("icon.svg"), Some("image/svg+xml"));
        assert_eq!(guess_content_type("report.pdf"), Some("application/pdf"));
        // Only the final extension matters (a dotted name isn't confused).
        assert_eq!(
            guess_content_type("archive.tar.gz"),
            Some("application/gzip")
        );
        assert_eq!(guess_content_type("v1.2.3.txt"), Some("text/plain"));
    }

    #[test]
    fn unknown_or_missing_extension_is_none() {
        // Caller defaults these to application/octet-stream.
        assert_eq!(guess_content_type("blob.bin"), None);
        assert_eq!(guess_content_type("README"), None);
        // A trailing dot (or empty extension) is not a known type.
        assert_eq!(guess_content_type("weird."), None);
        // A dotfile is a name, not an extension — no false positive.
        assert_eq!(guess_content_type(".gitignore"), None);
    }
}
