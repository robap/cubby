//! SQLite metadata store.
//!
//! SQLite is the source of truth for what exists; the filesystem only holds
//! bytes. Object rows are `WITHOUT ROWID` with `PK(bucket, key)` so the table
//! itself is the clustered index a future ListObjectsV2 scans in key order.
//!
//! rusqlite is synchronous, so the [`Connection`] lives behind a `Mutex` and
//! callers run queries inside `spawn_blocking`. A `busy_timeout` covers the
//! low concurrency of Phase 1 (see the plan's Risks).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

/// Handle to the metadata database. Cheap to clone (shared connection).
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open (creating if needed) the database at `path`, enable WAL, and apply
    /// the schema. Idempotent — safe to call on an existing database.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // WAL is a persistent property of the file; query form because the
        // pragma returns the resulting mode.
        let mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        debug_assert_eq!(mode.to_lowercase(), "wal");

        conn.execute_batch(SCHEMA_V0)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run a closure with locked access to the connection.
    pub(crate) fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        f(&conn)
    }

    /// Insert a bucket. Returns `true` if it was created, `false` if a bucket
    /// with that name already existed (idempotent — no error).
    pub fn create_bucket(&self, name: &str, created_at: i64) -> rusqlite::Result<bool> {
        self.with_conn(|c| {
            let n = c.execute(
                "INSERT OR IGNORE INTO buckets (name, created_at) VALUES (?1, ?2)",
                rusqlite::params![name, created_at],
            )?;
            Ok(n == 1)
        })
    }

    /// Whether a bucket row exists.
    pub fn bucket_exists(&self, name: &str) -> rusqlite::Result<bool> {
        self.with_conn(|c| {
            Ok(
                c.query_row("SELECT 1 FROM buckets WHERE name = ?1", [name], |_| Ok(()))
                    .optional()?
                    .is_some(),
            )
        })
    }

    /// Delete a bucket, but only if it exists and holds no objects. The check
    /// and delete run under one lock so the emptiness test can't race a
    /// concurrent put.
    pub fn try_delete_bucket(&self, name: &str) -> rusqlite::Result<DeleteBucketOutcome> {
        self.with_conn(|c| {
            let exists = c
                .query_row("SELECT 1 FROM buckets WHERE name = ?1", [name], |_| Ok(()))
                .optional()?
                .is_some();
            if !exists {
                return Ok(DeleteBucketOutcome::Missing);
            }
            let has_object = c
                .query_row(
                    "SELECT 1 FROM objects WHERE bucket = ?1 LIMIT 1",
                    [name],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if has_object {
                return Ok(DeleteBucketOutcome::NotEmpty);
            }
            c.execute("DELETE FROM buckets WHERE name = ?1", [name])?;
            Ok(DeleteBucketOutcome::Deleted)
        })
    }

    /// Insert or replace an object row. Called only after the bytes are already
    /// fsync'd and renamed into place, so the row is the authoritative record.
    pub fn put_object(&self, row: &ObjectRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO objects \
                 (bucket, key, size, etag, content_type, last_modified, metadata) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    row.bucket,
                    row.key,
                    row.size,
                    row.etag,
                    row.content_type,
                    row.last_modified,
                    row.metadata,
                ],
            )?;
            Ok(())
        })
    }

    /// Fetch an object's metadata row, or `None` if the key does not exist.
    pub fn get_object(&self, bucket: &str, key: &str) -> rusqlite::Result<Option<ObjectRow>> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT bucket, key, size, etag, content_type, last_modified, metadata \
                 FROM objects WHERE bucket = ?1 AND key = ?2",
                rusqlite::params![bucket, key],
                object_row_from,
            )
            .optional()
        })
    }

    /// Delete an object row. No error if the key does not exist (idempotent).
    pub fn delete_object(&self, bucket: &str, key: &str) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "DELETE FROM objects WHERE bucket = ?1 AND key = ?2",
                rusqlite::params![bucket, key],
            )?;
            Ok(())
        })
    }

    /// Delete every listed key for a bucket in one transaction. Idempotent per
    /// key (a never-existed key removes zero rows), matching the batch
    /// DeleteObjects contract; the caller unlinks the files afterward. An empty
    /// list is a no-op.
    pub fn delete_objects(&self, bucket: &str, keys: &[String]) -> rusqlite::Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            {
                let mut stmt = tx.prepare("DELETE FROM objects WHERE bucket = ?1 AND key = ?2")?;
                for key in keys {
                    stmt.execute(rusqlite::params![bucket, key])?;
                }
            }
            tx.commit()
        })
    }

    /// One index-backed page of a bucket's objects, in ascending key order.
    ///
    /// This is the sole seek primitive ListObjectsV2 stands on. The `objects`
    /// table is `WITHOUT ROWID` with `PK(bucket, key)`, so the table *is* the
    /// clustered index and this is a bounded range scan over it — no readdir,
    /// no whole-bucket load.
    ///
    /// - `prefix` restricts to keys in `[prefix, successor(prefix))`. Empty
    ///   prefix means the whole bucket.
    /// - `from` is an **inclusive** lower bound (`key >= from`): the resume
    ///   cursor. `None` starts at the beginning of the prefix range. The engine
    ///   encodes "strictly after key K" as `from = "K\0"` and "past group P" as
    ///   `from = successor(P)`, so a single inclusive bound serves both.
    /// - at most `limit` rows are returned; `limit == 0` yields an empty vec.
    ///
    /// Rows come back in BINARY (raw UTF-8 byte) order, exactly S3's order.
    pub fn list_objects_page(
        &self,
        bucket: &str,
        prefix: &str,
        from: Option<&str>,
        limit: i64,
    ) -> rusqlite::Result<Vec<ObjectRow>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        // Effective inclusive lower bound: the later of the resume cursor and
        // the prefix start. Both are `>=` constraints; taking the max keeps the
        // scan tight and lets a single bound cover the common empty-prefix case.
        let lower: &str = match from {
            Some(f) if f > prefix => f,
            _ => prefix,
        };
        let upper = crate::listing::successor(prefix); // None == scan to bucket end

        self.with_conn(|c| match upper {
            Some(upper) => {
                let mut stmt = c.prepare(
                    "SELECT bucket, key, size, etag, content_type, last_modified, metadata \
                     FROM objects \
                     WHERE bucket = ?1 AND key >= ?2 AND key < ?3 \
                     ORDER BY key LIMIT ?4",
                )?;
                let rows = stmt.query_map(
                    rusqlite::params![bucket, lower, upper, limit],
                    object_row_from,
                )?;
                rows.collect()
            }
            None => {
                let mut stmt = c.prepare(
                    "SELECT bucket, key, size, etag, content_type, last_modified, metadata \
                     FROM objects \
                     WHERE bucket = ?1 AND key >= ?2 \
                     ORDER BY key LIMIT ?3",
                )?;
                let rows =
                    stmt.query_map(rusqlite::params![bucket, lower, limit], object_row_from)?;
                rows.collect()
            }
        })
    }

    /// Total number of buckets. Cheap chrome for the web UI's health payload.
    pub fn count_buckets(&self) -> rusqlite::Result<i64> {
        self.with_conn(|c| c.query_row("SELECT COUNT(*) FROM buckets", [], |r| r.get(0)))
    }

    /// Total number of objects across all buckets. Cheap chrome for the web
    /// UI's health payload / nav footer.
    pub fn count_objects(&self) -> rusqlite::Result<i64> {
        self.with_conn(|c| c.query_row("SELECT COUNT(*) FROM objects", [], |r| r.get(0)))
    }

    /// All buckets, `(name, created_at)` in lexicographic name order.
    pub fn list_buckets(&self) -> rusqlite::Result<Vec<BucketRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare("SELECT name, created_at FROM buckets ORDER BY name")?;
            let rows = stmt.query_map([], |r| {
                Ok(BucketRow {
                    name: r.get(0)?,
                    created_at: r.get(1)?,
                })
            })?;
            rows.collect()
        })
    }

    /// All buckets with their object count and total byte size, in name order —
    /// the per-bucket column the web UI's bucket browser shows. A `LEFT JOIN`
    /// keeps empty buckets (count 0, size 0).
    pub fn list_buckets_with_stats(&self) -> rusqlite::Result<Vec<BucketStats>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT b.name, b.created_at, COUNT(o.key), COALESCE(SUM(o.size), 0) \
                 FROM buckets b LEFT JOIN objects o ON o.bucket = b.name \
                 GROUP BY b.name, b.created_at ORDER BY b.name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(BucketStats {
                    name: r.get(0)?,
                    created_at: r.get(1)?,
                    object_count: r.get(2)?,
                    total_size: r.get(3)?,
                })
            })?;
            rows.collect()
        })
    }

    /// Flat substring search over full object keys (`key LIKE '%term%'`),
    /// optionally scoped to one bucket, ascending by `(bucket, key)`, capped at
    /// `limit`. This is a **UI/seam convenience, not an S3 capability** (S3 lists
    /// by prefix only); a leading-wildcard `LIKE` can't use the clustered
    /// `(bucket, key)` index, so it is a full scan — acceptable for a dev tool.
    /// `limit <= 0` yields an empty vec.
    pub fn search_objects(
        &self,
        bucket: Option<&str>,
        term: &str,
        limit: i64,
    ) -> rusqlite::Result<Vec<ObjectRow>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        // Escape the LIKE metacharacters so a term such as `50%` or `a_b`
        // matches literally rather than as a wildcard.
        let pattern = format!("%{}%", escape_like(term));
        self.with_conn(|c| match bucket {
            Some(b) => {
                let mut stmt = c.prepare(
                    "SELECT bucket, key, size, etag, content_type, last_modified, metadata \
                     FROM objects WHERE bucket = ?1 AND key LIKE ?2 ESCAPE '\\' \
                     ORDER BY bucket, key LIMIT ?3",
                )?;
                let rows = stmt.query_map(rusqlite::params![b, pattern, limit], object_row_from)?;
                rows.collect()
            }
            None => {
                let mut stmt = c.prepare(
                    "SELECT bucket, key, size, etag, content_type, last_modified, metadata \
                     FROM objects WHERE key LIKE ?1 ESCAPE '\\' \
                     ORDER BY bucket, key LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![pattern, limit], object_row_from)?;
                rows.collect()
            }
        })
    }

    // --- Multipart uploads --------------------------------------------------

    /// Record a new in-flight multipart upload. The `upload_id` is the primary
    /// key and also names the `.multipart/<upload_id>/` staging directory.
    pub fn create_multipart(&self, row: &MultipartRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO multipart_uploads \
                 (upload_id, bucket, key, content_type, metadata, started_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    row.upload_id,
                    row.bucket,
                    row.key,
                    row.content_type,
                    row.metadata,
                    row.started_at,
                ],
            )?;
            Ok(())
        })
    }

    /// Fetch an in-flight upload's row, or `None` if the id is unknown (never
    /// created, or already completed/aborted).
    pub fn get_multipart(&self, upload_id: &str) -> rusqlite::Result<Option<MultipartRow>> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT upload_id, bucket, key, content_type, metadata, started_at \
                 FROM multipart_uploads WHERE upload_id = ?1",
                [upload_id],
                |r| {
                    Ok(MultipartRow {
                        upload_id: r.get(0)?,
                        bucket: r.get(1)?,
                        key: r.get(2)?,
                        content_type: r.get(3)?,
                        metadata: r.get(4)?,
                        started_at: r.get(5)?,
                    })
                },
            )
            .optional()
        })
    }

    /// Record (or replace) an uploaded part. Re-uploading a part number
    /// overwrites its recorded size and ETag (last write wins).
    pub fn put_part(
        &self,
        upload_id: &str,
        part_number: i32,
        size: i64,
        etag: &str,
    ) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO multipart_parts \
                 (upload_id, part_number, size, etag) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![upload_id, part_number, size, etag],
            )?;
            Ok(())
        })
    }

    /// All recorded parts of an upload, ascending by part number. Used by
    /// Complete to validate the client's list and assemble.
    pub fn all_parts(&self, upload_id: &str) -> rusqlite::Result<Vec<PartRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT part_number, size, etag FROM multipart_parts \
                 WHERE upload_id = ?1 ORDER BY part_number",
            )?;
            let rows = stmt.query_map([upload_id], part_row_from)?;
            rows.collect()
        })
    }

    /// One ascending page of parts strictly after `after` (the
    /// `part-number-marker`), at most `limit` rows. `after = None` starts at the
    /// first part; `limit <= 0` yields an empty vec.
    pub fn list_parts(
        &self,
        upload_id: &str,
        after: Option<i32>,
        limit: i64,
    ) -> rusqlite::Result<Vec<PartRow>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let after = after.unwrap_or(0);
        self.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT part_number, size, etag FROM multipart_parts \
                 WHERE upload_id = ?1 AND part_number > ?2 \
                 ORDER BY part_number LIMIT ?3",
            )?;
            let rows = stmt.query_map(rusqlite::params![upload_id, after, limit], part_row_from)?;
            rows.collect()
        })
    }

    /// Atomically finish an upload: insert the assembled object row and delete
    /// the multipart bookkeeping (parts + upload) in one transaction. The bytes
    /// are already fsync'd and renamed into place, so the object row is the
    /// authoritative record; running it with the deletes in one txn means a
    /// completed upload never leaves half its rows behind.
    pub fn complete_multipart(&self, upload_id: &str, object: &ObjectRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT OR REPLACE INTO objects \
                 (bucket, key, size, etag, content_type, last_modified, metadata) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    object.bucket,
                    object.key,
                    object.size,
                    object.etag,
                    object.content_type,
                    object.last_modified,
                    object.metadata,
                ],
            )?;
            tx.execute(
                "DELETE FROM multipart_parts WHERE upload_id = ?1",
                [upload_id],
            )?;
            tx.execute(
                "DELETE FROM multipart_uploads WHERE upload_id = ?1",
                [upload_id],
            )?;
            tx.commit()
        })
    }

    /// Discard an upload (abort): delete its parts and upload row. No object is
    /// created. Idempotent — deleting an already-gone id removes zero rows.
    pub fn delete_multipart(&self, upload_id: &str) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM multipart_parts WHERE upload_id = ?1",
                [upload_id],
            )?;
            tx.execute(
                "DELETE FROM multipart_uploads WHERE upload_id = ?1",
                [upload_id],
            )?;
            tx.commit()
        })
    }
}

/// Build a [`PartRow`] from a `SELECT part_number, size, etag` row.
fn part_row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<PartRow> {
    Ok(PartRow {
        part_number: r.get(0)?,
        size: r.get(1)?,
        etag: r.get(2)?,
    })
}

/// Build an [`ObjectRow`] from a `SELECT bucket, key, size, etag, content_type,
/// last_modified, metadata` row (column order fixed).
fn object_row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectRow> {
    Ok(ObjectRow {
        bucket: r.get(0)?,
        key: r.get(1)?,
        size: r.get(2)?,
        etag: r.get(3)?,
        content_type: r.get(4)?,
        last_modified: r.get(5)?,
        metadata: r.get(6)?,
    })
}

/// A row of the `buckets` table.
#[derive(Debug, Clone)]
pub struct BucketRow {
    pub name: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// A bucket plus aggregate stats, for the web UI's bucket column.
#[derive(Debug, Clone)]
pub struct BucketStats {
    pub name: String,
    /// Unix seconds.
    pub created_at: i64,
    pub object_count: i64,
    pub total_size: i64,
}

/// Escape SQL `LIKE` metacharacters (`\`, `%`, `_`) so a search term matches
/// literally under `ESCAPE '\'`.
fn escape_like(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for ch in term.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// A row of the `objects` table.
#[derive(Debug, Clone)]
pub struct ObjectRow {
    pub bucket: String,
    pub key: String,
    /// Size in bytes.
    pub size: i64,
    /// Hex MD5 (unquoted).
    pub etag: String,
    pub content_type: Option<String>,
    /// Unix seconds.
    pub last_modified: i64,
    /// User metadata as a JSON object string.
    pub metadata: String,
}

/// A row of the `multipart_uploads` table: an in-flight upload's captured
/// destination and headers.
#[derive(Debug, Clone)]
pub struct MultipartRow {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    /// Content type captured at CreateMultipartUpload (the object gets it).
    pub content_type: Option<String>,
    /// User metadata as a JSON object string, captured at Create.
    pub metadata: String,
    /// Unix seconds.
    pub started_at: i64,
}

/// A row of the `multipart_parts` table: one uploaded part.
#[derive(Debug, Clone)]
pub struct PartRow {
    pub part_number: i32,
    /// Size in bytes.
    pub size: i64,
    /// Hex MD5 (unquoted) of this part's bytes.
    pub etag: String,
}

/// Result of [`Db::try_delete_bucket`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteBucketOutcome {
    /// No such bucket.
    Missing,
    /// Bucket still holds objects.
    NotEmpty,
    /// Bucket row deleted.
    Deleted,
}

/// Schema version 0. `IF NOT EXISTS` so `open` is idempotent.
const SCHEMA_V0: &str = "\
CREATE TABLE IF NOT EXISTS buckets (
    name       TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS objects (
    bucket        TEXT    NOT NULL,
    key           TEXT    NOT NULL,
    size          INTEGER NOT NULL,
    etag          TEXT    NOT NULL,
    content_type  TEXT,
    last_modified INTEGER NOT NULL,
    metadata      TEXT    NOT NULL DEFAULT '{}',
    PRIMARY KEY (bucket, key)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS multipart_uploads (
    upload_id    TEXT    PRIMARY KEY,
    bucket       TEXT    NOT NULL,
    key          TEXT    NOT NULL,
    content_type TEXT,
    metadata     TEXT    NOT NULL DEFAULT '{}',
    started_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS multipart_parts (
    upload_id   TEXT    NOT NULL,
    part_number INTEGER NOT NULL,
    size        INTEGER NOT NULL,
    etag        TEXT    NOT NULL,
    PRIMARY KEY (upload_id, part_number)
) WITHOUT ROWID;
";

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (tempfile::TempDir, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("meta.sqlite")).unwrap();
        (tmp, db)
    }

    #[test]
    fn journal_mode_is_wal() {
        let (_tmp, db) = open_temp();
        let mode: String = db
            .with_conn(|c| c.query_row("PRAGMA journal_mode", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn buckets_and_objects_tables_exist() {
        let (_tmp, db) = open_temp();
        let names: Vec<String> = db
            .with_conn(|c| {
                let mut stmt =
                    c.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect()
            })
            .unwrap();
        assert!(names.contains(&"buckets".to_string()));
        assert!(names.contains(&"objects".to_string()));
    }

    #[test]
    fn objects_is_without_rowid() {
        let (_tmp, db) = open_temp();
        let sql: String = db
            .with_conn(|c| {
                c.query_row(
                    "SELECT sql FROM sqlite_master WHERE name='objects'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert!(
            sql.to_uppercase().contains("WITHOUT ROWID"),
            "objects must be WITHOUT ROWID: {sql}"
        );
        // A WITHOUT ROWID table has no implicit rowid.
        let no_rowid = db.with_conn(|c| c.query_row("SELECT rowid FROM objects", [], |_| Ok(())));
        assert!(
            no_rowid.is_err(),
            "WITHOUT ROWID table should not expose rowid"
        );
    }

    /// Insert a bare object row (only key matters for listing-order tests).
    fn seed(db: &Db, bucket: &str, key: &str) {
        db.put_object(&ObjectRow {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            size: 0,
            etag: "d41d8cd98f00b204e9800998ecf8427e".to_owned(),
            content_type: None,
            last_modified: 0,
            metadata: "{}".to_owned(),
        })
        .unwrap();
    }

    fn keys(db: &Db, bucket: &str, prefix: &str, from: Option<&str>, limit: i64) -> Vec<String> {
        db.list_objects_page(bucket, prefix, from, limit)
            .unwrap()
            .into_iter()
            .map(|r| r.key)
            .collect()
    }

    #[test]
    fn list_page_orders_by_binary_utf8() {
        let (_tmp, db) = open_temp();
        // Insert deliberately out of order; expect BINARY byte order back:
        // 'A'(0x41) < '_'(0x5F) < 'a'(0x61) < '~'(0x7E).
        for k in ["~tilde", "a", "A", "_under"] {
            seed(&db, "b", k);
        }
        assert_eq!(
            keys(&db, "b", "", None, 100),
            ["A", "_under", "a", "~tilde"]
        );
    }

    #[test]
    fn list_page_prefix_bound_excludes_non_matches() {
        let (_tmp, db) = open_temp();
        for k in ["notes.txt", "photos/a", "photos/b", "photoz", "q"] {
            seed(&db, "b", k);
        }
        // Only keys in ["photos/", successor) — "photoz" (0x7A > '/') is out.
        assert_eq!(
            keys(&db, "b", "photos/", None, 100),
            ["photos/a", "photos/b"]
        );
    }

    #[test]
    fn list_page_from_is_inclusive_lower_bound() {
        let (_tmp, db) = open_temp();
        for k in ["k0", "k1", "k2", "k3"] {
            seed(&db, "b", k);
        }
        // `from` is inclusive: "k1" is returned.
        assert_eq!(keys(&db, "b", "", Some("k1"), 100), ["k1", "k2", "k3"]);
        // "strictly after k1" is expressed as from = "k1\0".
        assert_eq!(keys(&db, "b", "", Some("k1\0"), 100), ["k2", "k3"]);
    }

    #[test]
    fn list_page_respects_limit() {
        let (_tmp, db) = open_temp();
        for k in ["k0", "k1", "k2", "k3", "k4"] {
            seed(&db, "b", k);
        }
        assert_eq!(keys(&db, "b", "", None, 2), ["k0", "k1"]);
        assert!(keys(&db, "b", "", None, 0).is_empty());
    }

    #[test]
    fn list_page_is_scoped_to_the_bucket() {
        let (_tmp, db) = open_temp();
        seed(&db, "a", "shared");
        seed(&db, "b", "shared");
        seed(&db, "b", "only-b");
        assert_eq!(keys(&db, "b", "", None, 100), ["only-b", "shared"]);
    }

    #[test]
    fn open_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("meta.sqlite");
        let _ = Db::open(&path).unwrap();
        // Second open must not fail on existing tables.
        let _ = Db::open(&path).unwrap();
    }

    #[test]
    fn delete_objects_removes_all_given_keys_in_one_call() {
        let (_tmp, db) = open_temp();
        for k in ["k1", "k2", "k3", "keep"] {
            seed(&db, "b", k);
        }
        // A never-existed key in the batch is a harmless no-op (idempotent).
        db.delete_objects(
            "b",
            &["k1".into(), "k2".into(), "k3".into(), "ghost".into()],
        )
        .unwrap();
        assert!(db.get_object("b", "k1").unwrap().is_none());
        assert!(db.get_object("b", "k2").unwrap().is_none());
        assert!(db.get_object("b", "k3").unwrap().is_none());
        // Unlisted keys survive.
        assert!(db.get_object("b", "keep").unwrap().is_some());
    }

    #[test]
    fn delete_objects_empty_list_is_noop() {
        let (_tmp, db) = open_temp();
        seed(&db, "b", "keep");
        db.delete_objects("b", &[]).unwrap();
        assert!(db.get_object("b", "keep").unwrap().is_some());
    }

    #[test]
    fn delete_objects_is_scoped_to_the_bucket() {
        let (_tmp, db) = open_temp();
        seed(&db, "a", "shared");
        seed(&db, "b", "shared");
        db.delete_objects("b", &["shared".into()]).unwrap();
        // Same key in another bucket is untouched.
        assert!(db.get_object("a", "shared").unwrap().is_some());
        assert!(db.get_object("b", "shared").unwrap().is_none());
    }

    #[test]
    fn counts_reflect_rows() {
        let (_tmp, db) = open_temp();
        assert_eq!(db.count_buckets().unwrap(), 0);
        assert_eq!(db.count_objects().unwrap(), 0);
        db.create_bucket("b", 0).unwrap();
        db.create_bucket("c", 0).unwrap();
        seed(&db, "b", "k1");
        seed(&db, "b", "k2");
        seed(&db, "c", "k1");
        assert_eq!(db.count_buckets().unwrap(), 2);
        assert_eq!(db.count_objects().unwrap(), 3);
    }

    #[test]
    fn list_buckets_with_stats_counts_and_sizes() {
        let (_tmp, db) = open_temp();
        db.create_bucket("empty", 10).unwrap();
        db.create_bucket("full", 20).unwrap();
        db.put_object(&ObjectRow {
            bucket: "full".into(),
            key: "a".into(),
            size: 100,
            etag: "e".into(),
            content_type: None,
            last_modified: 0,
            metadata: "{}".into(),
        })
        .unwrap();
        db.put_object(&ObjectRow {
            bucket: "full".into(),
            key: "b".into(),
            size: 40,
            etag: "e".into(),
            content_type: None,
            last_modified: 0,
            metadata: "{}".into(),
        })
        .unwrap();

        let stats = db.list_buckets_with_stats().unwrap();
        assert_eq!(stats.len(), 2);
        // Name order: "empty" < "full".
        assert_eq!(stats[0].name, "empty");
        assert_eq!(stats[0].object_count, 0);
        assert_eq!(stats[0].total_size, 0);
        assert_eq!(stats[1].name, "full");
        assert_eq!(stats[1].object_count, 2);
        assert_eq!(stats[1].total_size, 140);
    }

    #[test]
    fn search_objects_is_substring_scoped_and_capped() {
        let (_tmp, db) = open_temp();
        for (b, k) in [
            ("demo", "a/report.pdf"),
            ("demo", "logs/report-2.txt"),
            ("demo", "photos/cat.jpg"),
            ("other", "report.csv"),
        ] {
            seed(&db, b, k);
        }

        // Substring, not prefix: `port` matches mid-key in `a/report.pdf`.
        let hits = db.search_objects(Some("demo"), "port", 100).unwrap();
        let keys: Vec<&str> = hits.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["a/report.pdf", "logs/report-2.txt"]);

        // Global (no bucket) spans buckets.
        let hits = db.search_objects(None, "report", 100).unwrap();
        assert_eq!(hits.len(), 3);
        assert!(hits
            .iter()
            .any(|r| r.bucket == "other" && r.key == "report.csv"));

        // Cap applies (and one extra row can signal truncation to the caller).
        let capped = db.search_objects(None, "report", 1).unwrap();
        assert_eq!(capped.len(), 1);
        // limit <= 0 → empty.
        assert!(db.search_objects(None, "report", 0).unwrap().is_empty());
    }

    #[test]
    fn search_objects_escapes_like_metacharacters() {
        let (_tmp, db) = open_temp();
        seed(&db, "b", "50%off.txt");
        seed(&db, "b", "50somethingoff.txt");
        // `%` in the term must match literally, not as a wildcard.
        let hits = db.search_objects(Some("b"), "50%off", 100).unwrap();
        let keys: Vec<&str> = hits.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["50%off.txt"]);
    }

    #[test]
    fn multipart_tables_exist() {
        let (_tmp, db) = open_temp();
        let names: Vec<String> = db
            .with_conn(|c| {
                let mut stmt =
                    c.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect()
            })
            .unwrap();
        assert!(names.contains(&"multipart_uploads".to_string()));
        assert!(names.contains(&"multipart_parts".to_string()));
    }

    fn seed_upload(db: &Db, upload_id: &str) {
        db.create_multipart(&MultipartRow {
            upload_id: upload_id.to_owned(),
            bucket: "b".to_owned(),
            key: "big.bin".to_owned(),
            content_type: Some("text/plain".to_owned()),
            metadata: r#"{"x":"y"}"#.to_owned(),
            started_at: 123,
        })
        .unwrap();
    }

    #[test]
    fn get_multipart_returns_captured_content_type_and_metadata() {
        let (_tmp, db) = open_temp();
        seed_upload(&db, "u1");
        let row = db.get_multipart("u1").unwrap().expect("upload row exists");
        assert_eq!(row.bucket, "b");
        assert_eq!(row.key, "big.bin");
        assert_eq!(row.content_type.as_deref(), Some("text/plain"));
        assert_eq!(row.metadata, r#"{"x":"y"}"#);
        assert_eq!(row.started_at, 123);
        // Unknown id → None.
        assert!(db.get_multipart("nope").unwrap().is_none());
    }

    #[test]
    fn put_part_replaces_on_reupload() {
        let (_tmp, db) = open_temp();
        seed_upload(&db, "u1");
        db.put_part("u1", 1, 10, "aaaa").unwrap();
        db.put_part("u1", 1, 20, "bbbb").unwrap(); // re-upload same number
        let parts = db.all_parts("u1").unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].size, 20);
        assert_eq!(parts[0].etag, "bbbb");
    }

    #[test]
    fn all_parts_is_ascending() {
        let (_tmp, db) = open_temp();
        seed_upload(&db, "u1");
        db.put_part("u1", 5, 5, "e5").unwrap();
        db.put_part("u1", 1, 1, "e1").unwrap();
        db.put_part("u1", 9, 9, "e9").unwrap();
        let nums: Vec<i32> = db
            .all_parts("u1")
            .unwrap()
            .into_iter()
            .map(|p| p.part_number)
            .collect();
        assert_eq!(nums, [1, 5, 9]);
    }

    #[test]
    fn list_parts_paginates_strictly_after_marker() {
        let (_tmp, db) = open_temp();
        seed_upload(&db, "u1");
        for n in 1..=3 {
            db.put_part("u1", n, n as i64, "e").unwrap();
        }
        // First page of one part.
        let page1 = db.list_parts("u1", None, 1).unwrap();
        assert_eq!(page1.iter().map(|p| p.part_number).collect::<Vec<_>>(), [1]);
        // Resume strictly after part 1 → parts 2 and 3.
        let page2 = db.list_parts("u1", Some(1), 10).unwrap();
        assert_eq!(
            page2.iter().map(|p| p.part_number).collect::<Vec<_>>(),
            [2, 3]
        );
        // limit <= 0 → empty.
        assert!(db.list_parts("u1", None, 0).unwrap().is_empty());
    }

    #[test]
    fn complete_multipart_is_atomic_swap() {
        let (_tmp, db) = open_temp();
        seed_upload(&db, "u1");
        db.put_part("u1", 1, 10, "e1").unwrap();
        db.put_part("u1", 2, 20, "e2").unwrap();

        let object = ObjectRow {
            bucket: "b".to_owned(),
            key: "big.bin".to_owned(),
            size: 30,
            etag: "deadbeef-2".to_owned(),
            content_type: Some("text/plain".to_owned()),
            last_modified: 999,
            metadata: "{}".to_owned(),
        };
        db.complete_multipart("u1", &object).unwrap();

        // Object row created…
        let obj = db.get_object("b", "big.bin").unwrap().expect("object row");
        assert_eq!(obj.size, 30);
        assert_eq!(obj.etag, "deadbeef-2");
        // …and all multipart bookkeeping gone.
        assert!(db.get_multipart("u1").unwrap().is_none());
        assert!(db.all_parts("u1").unwrap().is_empty());
    }

    #[test]
    fn delete_multipart_removes_rows_without_object() {
        let (_tmp, db) = open_temp();
        seed_upload(&db, "u1");
        db.put_part("u1", 1, 10, "e1").unwrap();
        db.delete_multipart("u1").unwrap();
        assert!(db.get_multipart("u1").unwrap().is_none());
        assert!(db.all_parts("u1").unwrap().is_empty());
        assert!(db.get_object("b", "big.bin").unwrap().is_none());
    }
}
