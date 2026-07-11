//! Data-directory layout and bootstrap.
//!
//! A buckit data directory is self-contained: delete it for a factory reset,
//! copy it to clone an environment. Layout (per CONCEPT):
//!
//! ```text
//! <root>/
//!   .gitignore      # contains "*"
//!   meta.sqlite     # metadata / listing / multipart state
//!   buckets/        # object bytes, as real files
//!   .tmp/           # in-flight uploads (same FS → atomic rename)
//!   .multipart/     # {upload_id}/{part_number}
//! ```

use std::io;
use std::path::{Path, PathBuf};

/// A handle to a buckit data directory and the paths within it.
#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    /// Create a handle rooted at `root`. Does not touch the filesystem; call
    /// [`DataDir::bootstrap`] to create the layout.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The data directory root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/buckets` — where object bytes live as real files.
    pub fn buckets_dir(&self) -> PathBuf {
        self.root.join("buckets")
    }

    /// `<root>/.tmp` — in-flight uploads, on the same filesystem as `buckets`
    /// so the final rename is atomic.
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join(".tmp")
    }

    /// `<root>/.multipart` — staged multipart parts (used from Phase 3).
    pub fn multipart_dir(&self) -> PathBuf {
        self.root.join(".multipart")
    }

    /// `<root>/meta.sqlite` — the metadata database.
    pub fn meta_db_path(&self) -> PathBuf {
        self.root.join("meta.sqlite")
    }

    /// `<root>/.gitignore`.
    pub fn gitignore_path(&self) -> PathBuf {
        self.root.join(".gitignore")
    }

    /// `<root>/buckets/<name>` — the directory tree for one bucket's objects.
    pub fn bucket_dir(&self, name: &str) -> PathBuf {
        self.buckets_dir().join(name)
    }

    /// Create the directory layout, self-`.gitignore`, and an (empty)
    /// `meta.sqlite`. Idempotent: re-running never truncates existing content.
    pub fn bootstrap(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.buckets_dir())?;
        std::fs::create_dir_all(self.tmp_dir())?;
        std::fs::create_dir_all(self.multipart_dir())?;

        let gitignore = self.gitignore_path();
        if !gitignore.exists() {
            std::fs::write(&gitignore, "*\n")?;
        }

        // Establish the db file so the full layout exists after `serve`; the
        // schema is applied when the connection is opened. A zero-byte file is
        // a valid empty SQLite database.
        let db = self.meta_db_path();
        if !db.exists() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&db)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_creates_full_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::new(tmp.path().join("s3data"));

        dir.bootstrap().unwrap();

        assert!(dir.buckets_dir().is_dir());
        assert!(dir.tmp_dir().is_dir());
        assert!(dir.multipart_dir().is_dir());
        assert!(dir.meta_db_path().is_file());
        assert_eq!(
            std::fs::read_to_string(dir.gitignore_path()).unwrap(),
            "*\n"
        );
    }

    #[test]
    fn bootstrap_is_idempotent_and_preserves_content() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::new(tmp.path());

        dir.bootstrap().unwrap();
        // Simulate existing db content and a hand-edited gitignore.
        std::fs::write(dir.meta_db_path(), b"not empty").unwrap();
        std::fs::write(dir.gitignore_path(), "*\n!keep\n").unwrap();

        dir.bootstrap().unwrap();

        assert_eq!(std::fs::read(dir.meta_db_path()).unwrap(), b"not empty");
        assert_eq!(
            std::fs::read_to_string(dir.gitignore_path()).unwrap(),
            "*\n!keep\n"
        );
    }
}
