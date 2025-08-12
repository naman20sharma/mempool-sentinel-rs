use std::net::SocketAddr;

use reqwest::Client;
use tokio::task::JoinHandle;

use rust_eth_mempool_lab::api::{app_router, AppState};
use rust_eth_mempool_lab::ingest_stats::INGEST_STATS;
use rust_eth_mempool_lab::models::{BlockInfo, NormalizedTx};
use rust_eth_mempool_lab::storage::{self, DbPool};

#[tokio::test]
async fn health_endpoint_works() {
    let (base_url, handle) = spawn_app_with_data().await;
    let client = Client::new();
    let res = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body.get("status").and_then(|s| s.as_str()), Some("ok"));
    handle.abort();
}

#[tokio::test]
async fn top_senders_returns_data() {
    let (base_url, handle) = spawn_app_with_data().await;
    let client = Client::new();
    let res = client
        .get(format!("{}/stats/top-senders?limit=2", base_url))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    let arr = body
        .get("top_senders")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!arr.is_empty());
    handle.abort();
}

#[tokio::test]
async fn gas_stats_returns_numbers() {
    let (base_url, handle) = spawn_app_with_data().await;
    let client = Client::new();
    let res = client
        .get(format!("{}/stats/gas?blocks=10", base_url))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.get("min").is_some());
    assert!(body.get("max").is_some());
    assert!(body.get("avg").is_some());
    handle.abort();
}

#[tokio::test]
async fn ingest_stats_returns_counters() {
    let (base_url, handle) = spawn_app_with_data().await;
    INGEST_STATS.inc_blocks(1);
    INGEST_STATS.inc_transactions(2);
    let client = Client::new();
    let res = client
        .get(format!("{}/stats/ingest", base_url))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.get("blocks").is_some());
    assert!(body.get("transactions").is_some());
    assert!(body.get("pending_transactions").is_some());
    handle.abort();
}

#[tokio::test]
async fn recent_txs_returns_rows() {
    let (base_url, handle) = spawn_app_with_data().await;
    let client = Client::new();
    let res = client
        .get(format!("{}/tx/recent?limit=5", base_url))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().await.unwrap();
    let arr = body
        .get("transactions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!arr.is_empty());
    handle.abort();
}

async fn spawn_app_with_data() -> (String, JoinHandle<()>) {
    let db_url = temp_db_url();
    let pool = storage::init_pool(&db_url).await.unwrap();
    seed_data(&pool).await.unwrap();

    let state = AppState { pool: pool.clone() };
    let app = app_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);
    let server = axum::serve(listener, app);
    let handle = tokio::spawn(async move {
        let _ = server.await;
    });

    (base_url, handle)
}

fn temp_db_url() -> String {
    let dir = std::env::temp_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = format!(
        "rust_eth_mempool_lab_test_{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let path = dir.join(file);
    let _ = std::fs::File::create(&path);
    format!("sqlite://{}", path.to_string_lossy())
}

async fn seed_data(pool: &DbPool) -> anyhow::Result<()> {
    let block = BlockInfo {
        number: 1,
        hash: "0xabc".to_string(),
        timestamp: 1_700_000_000,
    };
    storage::insert_block(pool, &block).await?;

    let txs = vec![
        NormalizedTx {
            hash: "0xtx1".to_string(),
            from: "0xaaa".to_string(),
            to: Some("0xbbb".to_string()),
            value_wei: "1000000000000000000".to_string(),
            gas: 21_000,
            gas_price_wei: Some("1000".to_string()),
            max_fee_per_gas_wei: None,
            nonce: 1,
            block_number: Some(1),
            timestamp: Some(1_700_000_000),
            status: None,
        },
        NormalizedTx {
            hash: "0xtx2".to_string(),
            from: "0xccc".to_string(),
            to: Some("0xddd".to_string()),
            value_wei: "2000000000000000000".to_string(),
            gas: 30_000,
            gas_price_wei: Some("2000".to_string()),
            max_fee_per_gas_wei: None,
            nonce: 2,
            block_number: Some(1),
            timestamp: Some(1_700_000_005),
            status: None,
        },
    ];

    storage::insert_transactions(pool, &txs).await?;
    Ok(())
}
