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
}
