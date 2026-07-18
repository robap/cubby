//! Shared test harness: spawn an in-process cubby server on an ephemeral port
//! and hand back an `aws-sdk-s3` client pointed at it (path-style addressing).

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::{Client, Config};
use cubby::datadir::DataDir;
use cubby::db::Db;
use cubby::events::EventBus;
use cubby::http::{build_router, run_accept_loop, ServeConfig};
use tempfile::TempDir;
use tokio::net::TcpListener;

pub const ACCESS_KEY: &str = "local";
pub const SECRET_KEY: &str = "localsecret";

/// A running server against a temp data dir. Dropping it removes the data dir;
/// the accept task is aborted when the process ends (fine for tests).
pub struct TestServer {
    pub addr: SocketAddr,
    pub datadir: DataDir,
    /// The live-request event bus, so tests can subscribe and assert on the log.
    pub events: EventBus,
    _tmp: TempDir,
}

impl TestServer {
    pub async fn spawn() -> Self {
        Self::spawn_inner(None).await.expect("spawn server")
    }

    /// Spawn a server whose data dir has first been seeded from `seed_path`
    /// (applied through the real `seed::apply`, before serving) — the in-process
    /// analogue of `cubby serve <dir> --seed <seed_path>`. `file:` fixtures in
    /// the seed resolve against `seed_path`'s own directory.
    pub async fn spawn_seeded(seed_path: &std::path::Path) -> Self {
        Self::spawn_inner(Some(seed_path.to_owned()))
            .await
            .expect("spawn seeded server")
    }

    async fn spawn_inner(seed: Option<PathBuf>) -> anyhow::Result<Self> {
        let tmp = tempfile::tempdir().unwrap();
        let datadir = DataDir::new(tmp.path());
        datadir.bootstrap().unwrap();
        let db = Db::open(&datadir.meta_db_path()).unwrap();

        // Seeding happens before the listener is served, exactly as `serve()`
        // applies it before binding the port.
        if let Some(seed_path) = &seed {
            let store =
                cubby::store::Store::new(db.clone(), datadir.clone(), ACCESS_KEY.to_owned());
            cubby::seed::apply(seed_path, &store).await?;
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();

        let events = EventBus::new();
        let cfg = ServeConfig {
            bind: "127.0.0.1".to_owned(),
            port: 0,
            access_key: ACCESS_KEY.to_owned(),
            secret_key: SECRET_KEY.to_owned(),
            datadir: datadir.clone(),
            db,
            events: events.clone(),
            quiet: true,
            seed: None,
        };
        let router = build_router(&cfg);
        tokio::spawn(run_accept_loop(listener, router));

        Ok(Self {
            addr,
            datadir,
            events,
            _tmp: tmp,
        })
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
            .join(cubby::keypath::key_to_relpath(key))
    }

    /// Insert a synthetic object row directly (used before PutObject exists, to
    /// exercise "bucket not empty" paths). Writes over a second connection —
    /// WAL allows this concurrently with the server's connection.
    pub fn seed_object_row(&self, bucket: &str, key: &str) {
        self.seed_object_rows(bucket, std::iter::once(key));
    }

    /// Bulk-insert synthetic object rows in one transaction. Listing reads only
    /// SQLite, so this is a fast way to build a large fixture bucket without
    /// driving thousands of PutObject requests.
    pub fn seed_object_rows<'a>(&self, bucket: &str, keys: impl IntoIterator<Item = &'a str>) {
        let mut conn = rusqlite::Connection::open(self.datadir.meta_db_path()).unwrap();
        let tx = conn.transaction().unwrap();
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO objects \
                     (bucket, key, size, etag, last_modified, metadata) \
                     VALUES (?1, ?2, 0, 'd41d8cd98f00b204e9800998ecf8427e', 0, '{}')",
                )
                .unwrap();
            for key in keys {
                stmt.execute(rusqlite::params![bucket, key]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
}

// --- Recording webhook receiver ---------------------------------------------

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One request captured by a [`RecordingReceiver`].
#[derive(Clone)]
pub struct ReceivedRequest {
    pub path: String,
    pub body: Vec<u8>,
}

impl ReceivedRequest {
    /// Parse the body as JSON (panics if it isn't).
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("received body is JSON")
    }
}

/// A tiny in-test HTTP endpoint that records every POST cubby delivers to it —
/// the "local HTTP receiver" the acceptance criteria name. Optionally sleeps
/// before replying (to trip a destination's `timeout_ms`) and/or replies a
/// non-2xx status (to exercise log-and-drop).
#[derive(Clone)]
pub struct RecordingReceiver {
    pub addr: SocketAddr,
    received: Arc<Mutex<Vec<ReceivedRequest>>>,
}

impl RecordingReceiver {
    /// A receiver that replies `200` immediately.
    pub async fn spawn() -> Self {
        Self::spawn_with(0, 200).await
    }

    /// A receiver that sleeps `delay_ms` before replying `status`.
    pub async fn spawn_with(delay_ms: u64, status: u16) -> Self {
        use bytes::Bytes;
        use http_body_util::{BodyExt, Full};
        use hyper::body::Incoming;
        use hyper::service::service_fn;
        use hyper::{Request, Response, StatusCode};
        use hyper_util::rt::TokioIo;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received: Arc<Mutex<Vec<ReceivedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let store = received.clone();

        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let io = TokioIo::new(stream);
                let store = store.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let store = store.clone();
                        async move {
                            let path = req.uri().path().to_owned();
                            let body = req
                                .into_body()
                                .collect()
                                .await
                                .map(|c| c.to_bytes().to_vec())
                                .unwrap_or_default();
                            store.lock().unwrap().push(ReceivedRequest { path, body });
                            if delay_ms > 0 {
                                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            }
                            let code = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
                            Ok::<_, std::convert::Infallible>(
                                Response::builder()
                                    .status(code)
                                    .body(Full::new(Bytes::new()))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await;
                });
            }
        });

        Self { addr, received }
    }

    /// The URL a destination should POST to (`http://addr/hook`).
    pub fn url(&self) -> String {
        format!("http://{}/hook", self.addr)
    }

    /// How many requests have been received so far.
    pub fn count(&self) -> usize {
        self.received.lock().unwrap().len()
    }

    /// A snapshot of every received request.
    pub fn requests(&self) -> Vec<ReceivedRequest> {
        self.received.lock().unwrap().clone()
    }

    /// Wait until at least `n` requests have arrived (or `timeout` elapses),
    /// returning whether the count was reached. Polls so a delivery that never
    /// comes is bounded.
    pub async fn wait_for(&self, n: usize, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.count() >= n {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.count() >= n
    }
}
