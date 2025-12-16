mod api;
mod cli;
mod config;
mod eth;
mod ingest_stats;
mod models;
mod storage;

use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::config::Config;
use crate::eth::EthClient;
use crate::ingest_stats::INGEST_STATS;
use crate::models::NormalizedTx;
use std::collections::HashSet;

fn filter_txs(txs: &[NormalizedTx], filters: Option<&HashSet<String>>) -> Vec<NormalizedTx> {
    if let Some(filter) = filters {
        txs.iter()
            .filter(|tx| {
                let from_match = filter.contains(&tx.from);
                let to_match = tx
                    .to
                    .as_ref()
                    .map(|addr| filter.contains(addr))
                    .unwrap_or(false);
                from_match || to_match
            })
            .cloned()
            .collect()
    } else {
        txs.to_vec()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let cli = Cli::parse();
    let config = Config::from_env().context("failed to load configuration")?;

    match cli.command {
        Commands::Serve { addr } => {
            let bind = addr.unwrap_or_else(|| config.http_bind_addr.clone());
            let pool = storage::init_pool(&config.database_url).await?;
            api::run_http_server(&bind, pool).await?;
        }
        Commands::IngestOnce { blocks } => {
            tracing::info!("starting ingest-once for last {} blocks", blocks);

            let pool = storage::init_pool(&config.database_url).await?;
            let eth = EthClient::new(&config.eth_rpc_url)?;
            let blocks_with_txs = eth.fetch_recent_blocks(blocks).await?;

            let mut total_txs = 0usize;
            let mut total_blocks = 0usize;

            for (block_info, txs) in blocks_with_txs {
                let filtered = filter_txs(&txs, config.filter_addresses.as_ref());
                storage::insert_block(&pool, &block_info).await?;
                if !filtered.is_empty() {
                    storage::insert_transactions(&pool, &filtered).await?;
                    INGEST_STATS.inc_transactions(filtered.len() as u64);
                }
                total_txs += filtered.len();
                total_blocks += 1;
            }
            if total_blocks > 0 {
                INGEST_STATS.inc_blocks(total_blocks as u64);
            }

            tracing::info!(
                "ingest-once complete: inserted {} blocks, {} transactions",
                total_blocks,
                total_txs
            );
        }
        Commands::MempoolSample { duration_secs, max } => {
            let ws_url = config
                .eth_ws_url
                .as_deref()
                .ok_or_else(|| anyhow!("ETH_WS_URL must be set for mempool sampling"))?;

            let pool = storage::init_pool(&config.database_url).await?;
            let eth = EthClient::new(&config.eth_rpc_url)?;
            tracing::info!(
                "starting mempool sample: duration_secs={}, max={}",
                duration_secs,
                max
            );

            let stats = eth
                .sample_pending(
                    ws_url,
                    Duration::from_secs(duration_secs),
                    max as usize,
                    &pool,
                    config.filter_addresses.clone(),
                )
                .await?;

            tracing::info!(
                "mempool sample complete: received={}, fetched={}, inserted={}, insert_errors={}",
                stats.received,
                stats.fetched,
                stats.inserted,
                stats.insert_errors
            );
        }
        Commands::TopSenders { limit } => {
            let pool = storage::init_pool(&config.database_url).await?;
            let rows = storage::get_top_senders(&pool, limit as i64).await?;
            for row in rows {
                println!("{} {}", row.address, row.count);
            }
        }
        Commands::RecentTxs { limit } => {
            let pool = storage::init_pool(&config.database_url).await?;
            let txs = storage::get_recent_transactions(&pool, limit as i64).await?;
            for tx in txs {
                println!(
                    "{} from={} to={:?} value_wei={}",
                    tx.hash, tx.from, tx.to, tx.value_wei
                );
            }
        }
        Commands::GasStats { blocks } => {
            let pool = storage::init_pool(&config.database_url).await?;
            match storage::get_gas_stats(&pool, blocks as i64).await? {
                Some(stats) => println!(
                    "gas_price_wei min={} max={} avg={}",
                    stats.min, stats.max, stats.avg
                ),
                None => println!("no gas stats available"),
            }
        }
    }

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
}
