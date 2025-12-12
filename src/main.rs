mod api;
mod cli;
mod config;

use anyhow::Context;
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let cli = Cli::parse();
    let config = Config::from_env().context("failed to load configuration")?;

    match cli.command {
        Commands::Serve { addr } => {
            let bind = addr.unwrap_or_else(|| config.http_bind_addr.clone());
            api::run_http_server(&bind).await?;
        }
        Commands::IngestOnce { blocks } => {
            tracing::info!("ingest-once placeholder: last {} blocks", blocks);
        }
        Commands::MempoolSample { duration_secs } => {
            tracing::info!("mempool-sample placeholder: {}s", duration_secs);
        }
        Commands::TopSenders { limit } => {
            tracing::info!("top-senders placeholder: limit {}", limit);
        }
        Commands::GasStats { blocks } => {
            tracing::info!("gas-stats placeholder: last {} blocks", blocks);
        }
    }

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
}
