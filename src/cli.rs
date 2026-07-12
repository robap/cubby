//! Command-line interface.
//!
//! `cubby serve <dir>` is the only subcommand for now; it boots the S3 server
//! against a data directory, creating it (and its layout) on first run.

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
}

#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Data directory. Created (with its layout) if it does not exist.
    pub dir: PathBuf,

    /// Address to bind.
    #[arg(long, default_value = "127.0.0.1")]
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
        let Command::Serve(args) = cli.command;
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
        let Command::Serve(args) = cli.command;
        assert_eq!(args.seed, Some(PathBuf::from("seed.yaml")));
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
        let Command::Serve(args) = cli.command;
        assert_eq!(args.bind, "0.0.0.0");
        assert_eq!(args.port, 0);
        assert_eq!(args.access_key, "ak");
        assert_eq!(args.secret_key, "sk");
    }
}
