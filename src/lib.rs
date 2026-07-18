//! cubby — the SQLite of S3.
//!
//! A single-binary, S3-compatible object store for local development: bytes on
//! disk as real files, everything else in SQLite. This library exposes the
//! pieces the `cubby` binary wires together, and lets integration tests spawn
//! a server in-process on an ephemeral port.

pub mod access_log;
pub mod api;
pub mod banner;
pub mod cli;
pub mod cors;
pub mod datadir;
pub mod db;
pub mod embed;
pub mod events;
pub mod http;
pub mod keypath;
pub mod listing;
pub mod multipart;
pub mod notify;
pub mod presign;
pub mod seed;
pub mod store;
