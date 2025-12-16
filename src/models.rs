use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedTx {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value_wei: String,
    pub gas: i64,
    pub gas_price_wei: Option<String>,
    pub max_fee_per_gas_wei: Option<String>,
    pub nonce: i64,
    pub block_number: Option<i64>,
    pub timestamp: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub number: i64,
    pub hash: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GasStats {
    pub min: i64,
    pub max: i64,
    pub avg: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopSender {
    pub address: String,
    pub count: i64,
}
