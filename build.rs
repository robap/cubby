//! Build script: compile the `zero` web UI into `web/dist/` before the Rust
//! crate compiles, so `rust-embed` can embed it (see `src/embed.rs`).
//!
//! Per the Phase 5 decision, `web/dist/` is git-ignored and built fresh: the
//! binary always ships the current UI, and a stale `dist/` can never sneak in.
//! That makes `zero` a hard build dependency — if it is missing, or the build
//! fails, we abort loudly rather than embedding nothing.
//!
//! `zero.toml` lives at the repo root with `[project] root = "web"`, so every
//! `zero` command (including this one) runs from the crate root — no `cd`. The
//! framework-owned `web/.zero/` cache is git-ignored (zero's default) and fully
//! reproducible from `zero.toml` via `zero update -y`; we regenerate it when
//! absent so a fresh `git clone` + `cargo build` self-heals.
//!
//! We do **not** touch the built files. The SPA is served under `/_/` and its
//! root-absolute asset refs (`/assets/…`, `/.zero/…`) are rewritten to the
//! `/_/` mount prefix at **serve time** (`src/embed.rs`), not here — so a plain
//! `zero build` (which a UI dev runs directly) works identically to
//! `cargo build`, with no stale-rewrite footgun.

use std::path::Path;
use std::process::Command;

fn main() {
    // Rebuild the UI whenever its sources change. `zero build` output
    // (`web/dist/`) is regenerated each run, so we key off the inputs.
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/styles");
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=zero.toml");
    println!("cargo:rerun-if-changed=build.rs");

    // `web/.zero/` is git-ignored; regenerate it from `zero.toml` when absent so
    // `zero build` always has its framework inputs (fresh clone / CI checkout).
    if !Path::new("web/.zero").exists() {
        run(&["update", "--yes"]);
    }

    run(&["build"]);
}

fn run(args: &[&str]) {
    let what = format!("zero {}", args.join(" "));
    match Command::new("zero").args(args).status() {
        Ok(s) if s.success() => {}
        Ok(s) => panic!(
            "`{what}` failed with {s} — the web UI could not be built. Fix the \
             UI build (run `{what}` from the repo root to see the error) and retry."
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => panic!(
            "`zero` was not found on PATH. cubby embeds a `zero`-built web UI, \
             so `zero` is required to build. Install it with \
             `cargo install zero --locked` (the CLI at github.com/robap/zero) \
             and retry. Error: {e}"
        ),
        Err(e) => panic!("failed to run `{what}`: {e}"),
    }
}
