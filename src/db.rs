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
                |r| {
                    Ok(ObjectRow {
                        bucket: r.get(0)?,
                        key: r.get(1)?,
                        size: r.get(2)?,
                        etag: r.get(3)?,
                        content_type: r.get(4)?,
                        last_modified: r.get(5)?,
                        metadata: r.get(6)?,
                    })
                },
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

    #[test]
    fn open_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("meta.sqlite");
        let _ = Db::open(&path).unwrap();
        // Second open must not fail on existing tables.
        let _ = Db::open(&path).unwrap();
    }
}
