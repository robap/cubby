//! Command-line interface.
//!
//! `cubby serve <dir>` boots the S3 server against a data directory, creating it
//! (and its layout) on first run. `cubby reindex <dir>` is the offline
//! maintenance sibling: it scans `buckets/` and backfills `meta.sqlite` so
//! hand-dropped files become first-class objects, then exits without binding a
//! port.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Default access key when none is supplied.
pub const DEFAULT_ACCESS_KEY: &str = "local";
/// Default secret key when none is supplied.
pub const DEFAULT_SECRET_KEY: &str = "localsecret";

#[derive(Debug, Parser)]
#[command(
    name = "cubby",
    about = "The SQLite of S3 — a single-binary object store for local development",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Serve the S3 API against a data directory.
    Serve(ServeArgs),

    /// Scan `buckets/` and backfill `meta.sqlite` for hand-dropped files, then
    /// exit. Offline and additive: never binds a port, only inserts rows for
    /// files with no row (already-indexed objects are left untouched).
    Reindex(ReindexArgs),
}

#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Data directory. Created (with its layout) if it does not exist.
    pub dir: PathBuf,

    /// Address to bind. Defaults to loopback; `--bind 0.0.0.0` (or the
    /// `CUBBY_BIND` env) exposes it. The container image sets
    /// `CUBBY_BIND=0.0.0.0` so `-p` is reachable from the host regardless of
    /// the command args passed to `docker run`; an explicit `--bind` still wins.
    #[arg(long, env = "CUBBY_BIND", default_value = "127.0.0.1")]
    pub bind: String,

    /// Port to listen on. `0` binds an ephemeral port (printed machine-parseably).
    #[arg(long, default_value_t = 9000)]
    pub port: u16,

    /// Access key clients must present.
    #[arg(long, env = "CUBBY_ACCESS_KEY", default_value = DEFAULT_ACCESS_KEY)]
    pub access_key: String,

    /// Secret key clients must sign with.
    #[arg(long, env = "CUBBY_SECRET_KEY", default_value = DEFAULT_SECRET_KEY)]
    pub secret_key: String,

    /// Suppress the per-request stdout log line (useful in CI).
    #[arg(long)]
    pub quiet: bool,

    /// Seed file (YAML): buckets and fixture objects created on startup, before
    /// the port binds. A malformed seed fails fast without binding.
    #[arg(long, value_name = "FILE")]
    pub seed: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct ReindexArgs {
    /// Data directory to reindex. `bootstrap()`ed first (harmless, idempotent),
    /// so a bare tree holding only `buckets/<b>/<files>` and no `meta.sqlite`
    /// still works — the "rebuild the index from bytes" case.
    pub dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn serve_parses_dir_and_defaults() {
        let cli = Cli::try_parse_from(["cubby", "serve", "./s3data"]).unwrap();
        let Command::Serve(args) = cli.command else {
            panic!("expected serve subcommand");
        };
        assert_eq!(args.dir, PathBuf::from("./s3data"));
        assert_eq!(args.bind, "127.0.0.1");
        assert_eq!(args.port, 9000);
        assert_eq!(args.access_key, DEFAULT_ACCESS_KEY);
        assert_eq!(args.secret_key, DEFAULT_SECRET_KEY);
        // --seed is opt-in: absent means None (today's behavior, no seeding).
        assert_eq!(args.seed, None);
    }

    #[test]
    fn serve_parses_seed_path() {
        let cli = Cli::try_parse_from(["cubby", "serve", "data", "--seed", "seed.yaml"]).unwrap();
        let Command::Serve(args) = cli.command else {
            panic!("expected serve subcommand");
        };
        assert_eq!(args.seed, Some(PathBuf::from("seed.yaml")));
    }

    #[test]
    fn reindex_parses_dir() {
        let cli = Cli::try_parse_from(["cubby", "reindex", "./s3data"]).unwrap();
        let Command::Reindex(args) = cli.command else {
            panic!("expected reindex subcommand");
        };
        assert_eq!(args.dir, PathBuf::from("./s3data"));
    }

    #[test]
    fn reindex_takes_no_serve_flags() {
        // reindex is offline — bind/port/credentials are not accepted.
        assert!(Cli::try_parse_from(["cubby", "reindex", "data", "--port", "0"]).is_err());
    }

    #[test]
    fn serve_accepts_flag_overrides() {
        let cli = Cli::try_parse_from([
            "cubby",
            "serve",
            "data",
            "--bind",
            "0.0.0.0",
            "--port",
            "0",
            "--access-key",
            "ak",
            "--secret-key",
            "sk",
        ])
        .unwrap();
        let Command::Serve(args) = cli.command else {
            panic!("expected serve subcommand");
        };
        assert_eq!(args.bind, "0.0.0.0");
        assert_eq!(args.port, 0);
        assert_eq!(args.access_key, "ak");
        assert_eq!(args.secret_key, "sk");
    }
}
