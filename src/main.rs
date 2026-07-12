use clap::Parser;

use cubby::cli::{Cli, Command};
use cubby::datadir::DataDir;
use cubby::db::Db;
use cubby::events::EventBus;
use cubby::http::{serve, ServeConfig};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => {
            let dir = DataDir::new(&args.dir);
            dir.bootstrap()?;
            let db = Db::open(&dir.meta_db_path())?;

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(serve(ServeConfig {
                bind: args.bind,
                port: args.port,
                access_key: args.access_key,
                secret_key: args.secret_key,
                datadir: dir,
                db,
                events: EventBus::new(),
                quiet: args.quiet,
            }))
        }
    }
}
