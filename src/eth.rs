use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ethers_core::types::{Block, BlockId, Transaction, H160, H256, U256};
use ethers_providers::{Http, Middleware, Provider, Ws};
use futures_util::StreamExt;
use std::collections::HashSet;
use url::Url;

use crate::{
    ingest_stats::INGEST_STATS,
    models::{BlockInfo, NormalizedTx},
    storage::{self, DbPool},
};

#[derive(Clone)]
pub struct EthClient {
    provider: Provider<Http>,
}

#[derive(Debug, Default)]
pub struct PendingSampleStats {
    pub received: usize,
    pub fetched: usize,
    pub inserted: usize,
    pub insert_errors: usize,
}

impl EthClient {
    pub fn new(rpc_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .context("failed to build reqwest client")?;
        let url = Url::parse(rpc_url).context("invalid ETH_RPC_URL")?;
        let transport = Http::new_with_client(url, client);
        let provider = Provider::new(transport);
        Ok(Self { provider })
    }

    pub async fn fetch_recent_blocks(
        &self,
        count: u64,
    ) -> Result<Vec<(BlockInfo, Vec<NormalizedTx>)>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let latest = self
            .provider
            .get_block_number()
            .await
            .context("failed to fetch latest block number")?;

        let start = latest.saturating_sub((count - 1).into());
        let mut out = Vec::new();

        for num in start.as_u64()..=latest.as_u64() {
            let block_id = BlockId::Number(num.into());
            let maybe_block = self
                .provider
                .get_block_with_txs(block_id)
                .await
                .with_context(|| format!("failed to fetch block {}", num))?;

            if let Some(block) = maybe_block {
                if let Some(normalized) = normalize_block(block) {
                    out.push(normalized);
                    continue;
                }
            }

            // Fallback: fetch block hashes and hydrate transactions individually.
            let maybe_hash_block = self
                .provider
                .get_block(block_id)
                .await
                .with_context(|| format!("failed to fetch block {} (hash fallback)", num))?;
            if let Some(hash_block) = maybe_hash_block {
                if let (Some(number), Some(hash)) = (hash_block.number, hash_block.hash) {
                    let timestamp = hash_block.timestamp.as_u64() as i64;
                    let mut txs = Vec::new();
                    for tx_hash in hash_block.transactions {
                        if let Some(full_tx) = self.provider.get_transaction(tx_hash).await? {
                            txs.push(normalize_tx(full_tx, number.as_u64() as i64, timestamp));
                        }
                    }
                    let block_info = BlockInfo {
                        number: number.as_u64() as i64,
                        hash: format!("0x{:x}", hash),
                        timestamp,
                    };
                    out.push((block_info, txs));
                }
            }
        }

        Ok(out)
    }

    pub async fn sample_pending(
        &self,
        ws_url: &str,
        duration: Duration,
        max: usize,
        pool: &DbPool,
        filters: Option<HashSet<String>>,
    ) -> Result<PendingSampleStats> {
        let ws_provider = Provider::<Ws>::connect(ws_url)
            .await
            .context("failed to connect to ETH_WS_URL")?;
        let mut sub = ws_provider
            .subscribe_pending_txs()
            .await
            .context("failed to subscribe to pending txs")?;

        let mut stats = PendingSampleStats::default();
        let mut buffer: Vec<NormalizedTx> = Vec::new();

        let flush_every = 100usize;
        let deadline = Instant::now() + duration;

        while stats.received < max {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            let next = tokio::time::timeout(remaining, sub.next()).await;
            let Some(hash) = (match next {
                Ok(item) => item,
                Err(_) => break,
            }) else {
                break;
            };

            stats.received += 1;

            match self.provider.get_transaction(hash).await {
                Ok(Some(tx)) => {
                    stats.fetched += 1;
                    let normalized = normalize_pending_tx(tx);
                    if include_tx(&normalized, filters.as_ref()) {
                        buffer.push(normalized);
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!("failed to fetch pending tx {}: {}", hash, err);
                }
            }

            if buffer.len() >= flush_every {
                match storage::insert_transactions(pool, &buffer).await {
                    Ok(_) => {
                        stats.inserted += buffer.len();
                        INGEST_STATS.inc_pending_transactions(buffer.len() as u64);
                    }
                    Err(e) => {
                        stats.insert_errors += 1;
                        tracing::warn!("failed inserting pending tx batch: {}", e);
                    }
                }
                buffer.clear();
            }

            if stats.received >= max {
                break;
            }
        }

        if !buffer.is_empty() {
            match storage::insert_transactions(pool, &buffer).await {
                Ok(_) => {
                    stats.inserted += buffer.len();
                    INGEST_STATS.inc_pending_transactions(buffer.len() as u64);
                }
                Err(e) => {
                    stats.insert_errors += 1;
                    tracing::warn!("failed inserting final pending tx batch: {}", e);
                }
            }
        }

        Ok(stats)
    }
}

fn normalize_block(block: Block<Transaction>) -> Option<(BlockInfo, Vec<NormalizedTx>)> {
    let number: i64 = block.number?.as_u64() as i64;
    let hash: H256 = block.hash?;
    let timestamp = block.timestamp.as_u64() as i64;

    let block_info = BlockInfo {
        number,
        hash: format!("0x{:x}", hash),
        timestamp,
    };

    let txs = block
        .transactions
        .into_iter()
        .map(|tx| normalize_tx(tx, number, timestamp))
        .collect();

    Some((block_info, txs))
}

fn normalize_tx(tx: Transaction, block_number: i64, timestamp: i64) -> NormalizedTx {
    NormalizedTx {
        hash: format!("0x{:x}", tx.hash),
        from: address_to_lower_hex(tx.from),
        to: tx.to.map(address_to_lower_hex),
        value_wei: tx.value.to_string(),
        gas: u256_to_i64_lossy(tx.gas),
        gas_price_wei: tx.gas_price.map(|v| v.to_string()),
        max_fee_per_gas_wei: tx.max_fee_per_gas.map(|v| v.to_string()),
        nonce: u256_to_i64_lossy(tx.nonce),
        block_number: Some(block_number),
        timestamp: Some(timestamp),
        status: None,
    }
}

fn normalize_pending_tx(tx: Transaction) -> NormalizedTx {
    NormalizedTx {
        hash: format!("0x{:x}", tx.hash),
        from: address_to_lower_hex(tx.from),
        to: tx.to.map(address_to_lower_hex),
        value_wei: tx.value.to_string(),
        gas: u256_to_i64_lossy(tx.gas),
        gas_price_wei: tx.gas_price.map(|v| v.to_string()),
        max_fee_per_gas_wei: tx.max_fee_per_gas.map(|v| v.to_string()),
        nonce: u256_to_i64_lossy(tx.nonce),
        block_number: None,
        timestamp: None,
        status: None,
    }
}

fn include_tx(tx: &NormalizedTx, filters: Option<&HashSet<String>>) -> bool {
    if let Some(filter) = filters {
        let from_match = filter.contains(&tx.from);
        let to_match = tx
            .to
            .as_ref()
            .map(|addr| filter.contains(addr))
            .unwrap_or(false);
        from_match || to_match
    } else {
        true
    }
}

fn address_to_lower_hex(addr: H160) -> String {
    format!("0x{:x}", addr)
}

fn u256_to_i64_opt(value: U256) -> Option<i64> {
    let as_u128: u128 = value.try_into().ok()?;
    i64::try_from(as_u128).ok()
}

fn u256_to_i64_lossy(value: U256) -> i64 {
    u256_to_i64_opt(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers_core::types::U256;

    #[test]
    fn normalize_legacy_tx_sets_gas_price() {
        let mut tx = Transaction::default();
        tx.hash = H256::from_low_u64_be(1);
        tx.from = H160::from_low_u64_be(2);
        tx.to = Some(H160::from_low_u64_be(3));
        tx.value = U256::from(42u64);
        tx.gas = U256::from(21_000u64);
        tx.nonce = U256::from(7u64);
        tx.gas_price = Some(U256::from(1000u64));
        tx.max_fee_per_gas = None;

        let normalized = normalize_tx(tx, 10, 1234);
        assert_eq!(normalized.gas_price_wei, Some("1000".to_string()));
        assert_eq!(normalized.max_fee_per_gas_wei, None);
    }

    #[test]
    fn normalize_eip1559_tx_sets_max_fee() {
        let mut tx = Transaction::default();
        tx.hash = H256::from_low_u64_be(5);
        tx.from = H160::from_low_u64_be(6);
        tx.to = Some(H160::from_low_u64_be(7));
        tx.value = U256::from(99u64);
        tx.gas = U256::from(30_000u64);
        tx.nonce = U256::from(8u64);
        tx.gas_price = None;
        tx.max_fee_per_gas = Some(U256::from(2_000_000_000u64));

        let normalized = normalize_tx(tx, 11, 4567);
        assert_eq!(
            normalized.max_fee_per_gas_wei,
            Some("2000000000".to_string())
        );
        assert_eq!(normalized.gas_price_wei, None);
    }

    #[test]
    fn normalize_pending_tx_sets_block_fields_none() {
        let mut tx = Transaction::default();
        tx.hash = H256::from_low_u64_be(9);
        tx.from = H160::from_low_u64_be(10);
        tx.value = U256::from(123u64);
        tx.gas = U256::from(50_000u64);
        tx.nonce = U256::from(3u64);
        tx.gas_price = Some(U256::from(5000u64));

        let normalized = normalize_pending_tx(tx);
        assert_eq!(normalized.block_number, None);
        assert_eq!(normalized.timestamp, None);
        assert_eq!(normalized.gas_price_wei, Some("5000".to_string()));
    }
}
