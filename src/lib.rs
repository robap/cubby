//! buckit — the SQLite of S3.
//!
//! A single-binary, S3-compatible object store for local development: bytes on
//! disk as real files, everything else in SQLite. This library exposes the
//! pieces the `buckit` binary wires together, and lets integration tests spawn
//! a server in-process on an ephemeral port.

pub mod banner;
pub mod cli;
pub mod datadir;
pub mod db;
pub mod http;
pub mod keypath;
pub mod store;
