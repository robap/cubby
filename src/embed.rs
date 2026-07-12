//! Embedded web UI assets.
//!
//! `web/dist/` is a **committed** build artifact (the `zero` production build),
//! compiled into the binary with `rust-embed`, so `./cubby serve` ships the UI
//! with no Node, no network, and no separate process — and cubby builds with a
//! Rust toolchain alone, `zero` never on the build path. Regenerating the UI
//! (`zero build`) is a developer step done before committing, not part of
//! `cargo build`. In debug builds `rust-embed` reads `web/dist/` from disk
//! (nice for `cargo run`); release builds embed the bytes.
//!
//! Everything is served under the `/_/` mount point (see `http.rs`): real
//! assets get a long immutable cache header, and any non-asset `/_/…` path
//! falls back to `index.html` for client-side (SPA) routing.
//!
//! `zero` emits root-absolute asset refs (`/assets/…`, `/.zero/…`) with no
//! base-path config, so `index.html` and the CSS are rewritten to the `/_/`
//! prefix **as they are served** — not at build time. That keeps a bare
//! `zero build` (what a UI dev runs) working identically to what is embedded:
//! the on-disk `dist/` stays in `zero`'s native form and the prefixing happens
//! once, here, where it can't be bypassed or double-applied.

use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::{Response, StatusCode};
use rust_embed::RustEmbed;
use s3s::Body;

/// The `zero` production build, embedded from `web/dist/`.
#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct Assets;

/// The SPA entry document.
const INDEX_HTML: &str = "index.html";

/// Hashed bundle assets and fonts never change under a given URL, so they are
/// safe to cache forever.
const IMMUTABLE_CACHE: &str = "public, max-age=31536000, immutable";

/// `index.html` names the current hashed bundles, so it must not be cached.
const NO_CACHE: &str = "no-cache";

/// Serve the embedded asset at `rel_path` (relative to `web/dist/`), or `None`
/// if no such asset is embedded. Real assets get their content-type and a long
/// immutable cache header. Text assets that carry root-absolute refs (CSS →
/// `/.zero/fonts/…`) are rewritten to the `/_/` mount prefix.
pub fn serve_embedded(rel_path: &str) -> Option<Response<Body>> {
    let file = Assets::get(rel_path)?;
    let body = if needs_prefix_rewrite(rel_path) {
        match std::str::from_utf8(&file.data) {
            Ok(text) => Body::from(rewrite_mount_prefix(text)),
            Err(_) => Body::from(file.data.into_owned()),
        }
    } else {
        Body::from(file.data.into_owned())
    };
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type_for(rel_path))
        .header(CACHE_CONTROL, IMMUTABLE_CACHE)
        .body(body)
        .expect("asset response builds");
    Some(resp)
}

/// Serve `index.html` — the SPA shell, used for `/_/` and every client-side
/// route (SPA fallback). Not cached, so a rebuilt UI is picked up immediately.
/// Asset refs are rewritten to the `/_/` mount prefix (see module docs).
pub fn serve_index() -> Response<Body> {
    match Assets::get(INDEX_HTML) {
        Some(file) => {
            let html = match std::str::from_utf8(&file.data) {
                Ok(text) => rewrite_mount_prefix(text),
                Err(_) => String::from_utf8_lossy(&file.data).into_owned(),
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "text/html; charset=utf-8")
                .header(CACHE_CONTROL, NO_CACHE)
                .body(Body::from(html))
                .expect("index response builds")
        }
        // The build embeds index.html; its absence means a broken build.
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from(
                "web UI assets are missing from this build\n".to_owned(),
            ))
            .expect("error response builds"),
    }
}

/// Whether an embedded asset is a text file that may carry root-absolute refs
/// needing the `/_/` mount prefix. Only `index.html` and the CSS do; the JS
/// bundle has none (verified against `zero`'s output).
fn needs_prefix_rewrite(path: &str) -> bool {
    matches!(path.rsplit('.').next(), Some("html") | Some("css"))
}

/// Prefix `zero`'s root-absolute asset refs (`/assets/…`, `/.zero/…`) with the
/// `/_/` mount. Applied only to `zero`'s native (un-prefixed) output, so it is
/// a single, non-doubling pass.
fn rewrite_mount_prefix(body: &str) -> String {
    body.replace("/assets/", "/_/assets/")
        .replace("/.zero/", "/_/.zero/")
}

/// Guess a content-type from the asset's file extension. The UI emits a small,
/// fixed set of asset kinds; anything unrecognized is served as opaque bytes.
fn content_type_for(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "map" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_html_is_embedded_and_hosts_the_app_root() {
        // Proves the committed `web/dist/` is embedded by `rust-embed`.
        let file = Assets::get(INDEX_HTML).expect("index.html is embedded");
        let html = std::str::from_utf8(&file.data).unwrap();
        assert!(
            html.contains(r#"id="app""#),
            "index.html mounts #app: {html}"
        );
    }

    #[test]
    fn serving_rewrites_asset_paths_under_the_mount_prefix() {
        // The embedded (native `zero`) build references root-absolute /assets/,
        // and the serve layer rewrites them under /_/ so they resolve there.
        let file = Assets::get(INDEX_HTML).unwrap();
        let raw = std::str::from_utf8(&file.data).unwrap();
        let served = rewrite_mount_prefix(raw);
        assert!(
            served.contains("/_/assets/"),
            "served index references assets under /_/: {served}"
        );
        assert!(
            !served.contains("\"/assets/"),
            "no root-absolute /assets/ refs should remain after rewrite: {served}"
        );
    }

    #[test]
    fn rewrite_prefixes_both_asset_roots_and_is_otherwise_a_noop() {
        assert_eq!(rewrite_mount_prefix("<p>hi</p>"), "<p>hi</p>");
        assert_eq!(
            rewrite_mount_prefix(r#"<script src="/assets/app.js">"#),
            r#"<script src="/_/assets/app.js">"#
        );
        assert_eq!(
            rewrite_mount_prefix("url(/.zero/fonts/geist.woff2)"),
            "url(/_/.zero/fonts/geist.woff2)"
        );
    }

    #[test]
    fn only_text_assets_are_rewritten() {
        assert!(needs_prefix_rewrite("index.html"));
        assert!(needs_prefix_rewrite("assets/app.5dd6bdc5.css"));
        assert!(!needs_prefix_rewrite("assets/app.22b64df4.js"));
        assert!(!needs_prefix_rewrite(".zero/fonts/geist.woff2"));
    }

    #[test]
    fn serve_embedded_sets_content_type_and_cache() {
        let resp = serve_embedded(INDEX_HTML).expect("index served");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CACHE_CONTROL).unwrap(), IMMUTABLE_CACHE);
    }

    #[test]
    fn serve_embedded_misses_return_none() {
        assert!(serve_embedded("does/not/exist.js").is_none());
    }

    #[test]
    fn content_types_cover_the_asset_kinds() {
        assert_eq!(content_type_for("a.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type_for("a.css"), "text/css; charset=utf-8");
        assert_eq!(content_type_for("f.woff2"), "font/woff2");
        assert_eq!(content_type_for("x.unknown"), "application/octet-stream");
    }
}
