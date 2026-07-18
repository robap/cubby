//! A tiny webhook receiver you can point a cubby notification destination at, to
//! watch events land — the runnable counterpart to the notifications feature.
//!
//! ```text
//! cargo run --example webhook_sink -- --port 3000
//! ```
//!
//! It prints each received POST (method, path, and the pretty-printed JSON
//! body), and adds a one-line `parsed as S3 event ✓` note when the body is an
//! `s3-notification` envelope. Two flags exercise cubby's delivery semantics:
//!
//! - `--delay <ms>`  sleep this long before replying (to trip a destination's
//!   `timeout_ms` and prove the object mutation still returns promptly).
//! - `--status <code>`  reply with this HTTP status (e.g. `500`) to prove cubby
//!   logs-and-drops a non-2xx without retrying.
//!
//! Uses only cubby's normal dependencies (hyper + hyper-util, no TLS), so it
//! builds with the same toolchain and matches cubby's own http-only client.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use clap::Parser;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(
    name = "webhook_sink",
    about = "A dev webhook receiver for cubby event notifications"
)]
struct Args {
    /// Port to listen on (binds 127.0.0.1).
    #[arg(long, default_value_t = 3000)]
    port: u16,
    /// Milliseconds to sleep before replying (to trip a destination's timeout).
    #[arg(long, default_value_t = 0)]
    delay: u64,
    /// HTTP status to reply with (e.g. 500 to prove log-and-drop, no retry).
    #[arg(long, default_value_t = 200)]
    status: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = TcpListener::bind(addr).await?;
    println!(
        "webhook_sink listening on http://{addr}  (delay={}ms, reply status={})",
        args.delay, args.status
    );
    println!("point a cubby destination url here, e.g. http://{addr}/s3-hook\n");

    loop {
        let (stream, _peer) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let (delay, status) = (args.delay, args.status);
        tokio::spawn(async move {
            let service = service_fn(move |req: Request<Incoming>| async move {
                handle(req, delay, status).await
            });
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("connection error: {e}");
            }
        });
    }
}

/// Print one received request, then reply (after the optional delay) with the
/// configured status.
async fn handle(
    req: Request<Incoming>,
    delay: u64,
    status: u16,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let body = req
        .into_body()
        .collect()
        .await
        .map(|c| c.to_bytes())
        .unwrap_or_default();

    println!("── {method} {path}");
    match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(json) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string())
            );
            if let Some(key) = s3_object_key(&json) {
                println!("parsed as S3 event ✓  object key: {key}");
            }
        }
        // Not JSON — dump the raw bytes so nothing is hidden.
        Err(_) => println!("{}", String::from_utf8_lossy(&body)),
    }
    println!();

    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    Ok(Response::builder()
        .status(code)
        .body(Full::new(Bytes::new()))
        .expect("response builds"))
}

/// If `json` is an `s3-notification` envelope, return its first record's object
/// key — the marker that this parsed as a real AWS-shaped S3 event.
fn s3_object_key(json: &serde_json::Value) -> Option<&str> {
    json.get("Records")?
        .as_array()?
        .first()?
        .get("s3")?
        .get("object")?
        .get("key")?
        .as_str()
}
