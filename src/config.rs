use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub eth_rpc_url: String,
    pub database_url: String,
    pub http_bind_addr: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("missing ETH_RPC_URL env var")]
    MissingEthRpcUrl,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let eth_rpc_url =
            env::var("ETH_RPC_URL").map_err(|_| ConfigError::MissingEthRpcUrl)?;

        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/mempool.db".to_string());
        let http_bind_addr =
            env::var("HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

        Ok(Self {
            eth_rpc_url,
            database_url,
            http_bind_addr,
        })
    }
}
