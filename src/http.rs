//! HTTP layer: the routing skeleton and the hyper serve loop.
//!
//! One port, routed: `/_/*` is reserved for the web UI (Phase 5) and returns a
//! `501` placeholder here; everything else is handed to the `s3s` service,
//! which owns the S3 wire protocol, header SigV4, and XML. Underscore-prefixed
//! bucket names are illegal in S3, so there is zero namespace collision.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;

use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use s3s::auth::SimpleAuth;
use s3s::service::{S3Service, S3ServiceBuilder};
use s3s::{Body, HttpError, HttpResponse};
use tokio::net::TcpListener;

use crate::banner;
use crate::datadir::DataDir;
use crate::db::Db;
use crate::store::Store;

/// Everything needed to build and run the server.
pub struct ServeConfig {
    pub bind: String,
    pub port: u16,
    pub access_key: String,
    pub secret_key: String,
    pub datadir: DataDir,
    pub db: Db,
}

/// The top routing layer wrapping the `s3s` service.
#[derive(Clone)]
pub struct Router {
    s3: S3Service,
}

impl hyper::service::Service<hyper::Request<Incoming>> for Router {
    type Response = HttpResponse;
    type Error = HttpError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: hyper::Request<Incoming>) -> Self::Future {
        let path = req.uri().path();
        if path == "/_" || path.starts_with("/_/") {
            let resp = hyper::Response::builder()
                .status(hyper::StatusCode::NOT_IMPLEMENTED)
                .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from(
                    "buckit web UI is coming in Phase 5\n".to_owned(),
                ))
                .expect("static 501 response is valid");
            return Box::pin(async move { Ok(resp) });
        }
        // Delegate to the s3s hyper service (it maps `Incoming` → `s3s::Body`).
        hyper::service::Service::call(&self.s3, req)
    }
}

/// Build the routed service (S3 backend + fixed-credential SigV4 auth).
pub fn build_router(cfg: &ServeConfig) -> Router {
    let store = Store::new(cfg.db.clone(), cfg.datadir.clone(), cfg.access_key.clone());
    let mut builder = S3ServiceBuilder::new(store);
    builder.set_auth(SimpleAuth::from_single(
        cfg.access_key.clone(),
        cfg.secret_key.clone(),
    ));
    Router {
        s3: builder.build(),
    }
}

/// Accept connections forever, serving each with the router. Returns only on a
/// fatal accept error.
pub async fn run_accept_loop(listener: TcpListener, router: Router) -> std::io::Result<()> {
    loop {
        let (stream, _peer) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let router = router.clone();
        tokio::spawn(async move {
            if let Err(err) = ConnBuilder::new(TokioExecutor::new())
                .serve_connection(io, router)
                .await
            {
                tracing::debug!("connection error: {err}");
            }
        });
    }
}

/// Bind, print the banner, and serve until a fatal error. Used by `main`.
pub async fn serve(cfg: ServeConfig) -> anyhow::Result<()> {
    let listener = TcpListener::bind((cfg.bind.as_str(), cfg.port)).await?;
    let addr: SocketAddr = listener.local_addr()?;
    banner::print(addr, &cfg.access_key, &cfg.secret_key, cfg.datadir.root());
    let router = build_router(&cfg);
    run_accept_loop(listener, router).await?;
    Ok(())
}
