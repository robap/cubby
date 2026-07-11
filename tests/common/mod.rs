//! Shared test harness: spawn an in-process buckit server on an ephemeral port
//! and hand back an `aws-sdk-s3` client pointed at it (path-style addressing).

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::{Client, Config};
use buckit::datadir::DataDir;
use buckit::db::Db;
use buckit::http::{build_router, run_accept_loop, ServeConfig};
use tempfile::TempDir;
use tokio::net::TcpListener;

pub const ACCESS_KEY: &str = "local";
pub const SECRET_KEY: &str = "localsecret";

/// A running server against a temp data dir. Dropping it removes the data dir;
/// the accept task is aborted when the process ends (fine for tests).
pub struct TestServer {
    pub addr: SocketAddr,
    pub datadir: DataDir,
    _tmp: TempDir,
}

impl TestServer {
    pub async fn spawn() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let datadir = DataDir::new(tmp.path());
        datadir.bootstrap().unwrap();
        let db = Db::open(&datadir.meta_db_path()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();

        let cfg = ServeConfig {
            bind: "127.0.0.1".to_owned(),
            port: 0,
            access_key: ACCESS_KEY.to_owned(),
            secret_key: SECRET_KEY.to_owned(),
            datadir: datadir.clone(),
            db,
        };
        let router = build_router(&cfg);
        tokio::spawn(run_accept_loop(listener, router));

        Self {
            addr,
            datadir,
            _tmp: tmp,
        }
    }

    /// A client signing with the correct default credentials.
    pub fn client(&self) -> Client {
        self.client_with(ACCESS_KEY, SECRET_KEY)
    }

    /// A client signing with arbitrary credentials (for auth-failure tests).
    pub fn client_with(&self, access: &str, secret: &str) -> Client {
        let creds = Credentials::new(access, secret, None, None, "test");
        let conf = Config::builder()
            .region(Region::new("us-east-1"))
            .endpoint_url(format!("http://{}", self.addr))
            .credentials_provider(creds)
            .force_path_style(true)
            .behavior_version(BehaviorVersion::latest())
            .build();
        Client::from_conf(conf)
    }

    /// Absolute on-disk path where an object's bytes should live.
    pub fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.datadir
            .bucket_dir(bucket)
            .join(buckit::keypath::key_to_relpath(key))
    }

    /// Insert a synthetic object row directly (used before PutObject exists, to
    /// exercise "bucket not empty" paths). Writes over a second connection —
    /// WAL allows this concurrently with the server's connection.
    pub fn seed_object_row(&self, bucket: &str, key: &str) {
        let conn = rusqlite::Connection::open(self.datadir.meta_db_path()).unwrap();
        conn.execute(
            "INSERT INTO objects (bucket, key, size, etag, last_modified, metadata) \
             VALUES (?1, ?2, 0, 'seed', 0, '{}')",
            rusqlite::params![bucket, key],
        )
        .unwrap();
    }
}
