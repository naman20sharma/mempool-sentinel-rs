use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rust-eth-mempool-lab", version, about = "Ethereum mempool/block observer")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Fetch last N blocks and store transactions
    IngestOnce {
        #[arg(long, default_value_t = 5)]
        blocks: u64,
    },
    /// Sample pending txs for a duration (placeholder for now)
    MempoolSample {
        #[arg(long, default_value_t = 15)]
        duration_secs: u64,
    },
    /// Print top senders by tx count
    TopSenders {
        #[arg(long, default_value_t = 10)]
        limit: u64,
    },
    /// Gas price stats over last N blocks
    GasStats {
        #[arg(long, default_value_t = 10)]
        blocks: u64,
    },
    /// Run the HTTP API server
    Serve {
        /// Override bind address, e.g. 0.0.0.0:8080
        #[arg(long)]
        addr: Option<String>,
    },
}
