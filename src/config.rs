use std::collections::HashSet;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub eth_rpc_url: String,
    pub eth_ws_url: Option<String>,
    pub database_url: String,
    pub http_bind_addr: String,
    pub filter_addresses: Option<HashSet<String>>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("missing ETH_RPC_URL env var")]
    MissingEthRpcUrl,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let eth_rpc_url = env::var("ETH_RPC_URL").map_err(|_| ConfigError::MissingEthRpcUrl)?;
        let eth_ws_url = env::var("ETH_WS_URL").ok();

        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/mempool.db".to_string());
        let http_bind_addr = env::var("HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let filter_addresses = env::var("FILTER_ADDRESSES")
            .ok()
            .map(parse_filter_addresses)
            .and_then(|set| if set.is_empty() { None } else { Some(set) });

        Ok(Self {
            eth_rpc_url,
            eth_ws_url,
            database_url,
            http_bind_addr,
            filter_addresses,
        })
    }
}

fn parse_filter_addresses(raw: String) -> HashSet<String> {
    raw.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}
