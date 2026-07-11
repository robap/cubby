//! Startup banner.

use std::net::SocketAddr;
use std::path::Path;

/// Print the startup banner: S3 API URL, Web UI URL, credentials, data dir.
/// The API URL line is machine-parseable (the resolved `host:port`), so a test
/// harness binding `--port 0` can read the ephemeral port from stdout.
pub fn print(addr: SocketAddr, access_key: &str, secret_key: &str, data_dir: &Path) {
    let url = format!("http://{addr}");
    println!("  S3 API   → {url}   (access key: {access_key} / secret: {secret_key})");
    println!("  Web UI   → {url}/_/");
    println!("  Data dir → {}", data_dir.display());
}
