use std::path::Path;

use anyhow::{Context, Result};
use sqlx::{sqlite::SqlitePoolOptions, FromRow, Row, SqlitePool};
use tracing::warn;

use crate::models::{BlockInfo, GasStats, NormalizedTx, TopSender};

pub type DbPool = SqlitePool;

pub async fn init_pool(database_url: &str) -> Result<DbPool> {
    ensure_dir_exists(database_url)?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .context("failed to connect to SQLite")?;

    apply_schema(&pool).await?;
    Ok(pool)
}

fn ensure_dir_exists(database_url: &str) -> Result<()> {
    if let Some(path) = database_url.strip_prefix("sqlite://") {
        if path != ":memory:" {
            if let Some(dir) = Path::new(path).parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("failed creating db directory {:?}", dir))?;
            }
        }
    }
    Ok(())
}

async fn apply_schema(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blocks (
            block_number INTEGER PRIMARY KEY,
            block_hash TEXT NOT NULL,
            timestamp INTEGER NOT NULL
        );
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transactions (
            hash TEXT PRIMARY KEY,
            from_addr TEXT NOT NULL,
            to_addr TEXT,
            value_wei TEXT NOT NULL,
            gas INTEGER NOT NULL,
            gas_price_wei TEXT,
            max_fee_per_gas_wei TEXT,
            nonce INTEGER NOT NULL,
            block_number INTEGER,
            timestamp INTEGER,
            status TEXT,
            FOREIGN KEY(block_number) REFERENCES blocks(block_number)
        );
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_transactions_from_addr ON transactions(from_addr);
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_transactions_block_number ON transactions(block_number);
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_transactions_timestamp ON transactions(timestamp);
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_transactions_ts_coalesce
        ON transactions(COALESCE(timestamp, 0));
        "#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    verify_value_wei_column(pool).await?;
    Ok(())
}

pub async fn insert_block(pool: &SqlitePool, block: &BlockInfo) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO blocks (block_number, block_hash, timestamp)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(block_number) DO NOTHING;
        "#,
    )
    .bind(block.number)
    .bind(&block.hash)
    .bind(block.timestamp)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_transactions(pool: &SqlitePool, txs: &[NormalizedTx]) -> Result<()> {
    let mut txn = pool.begin().await?;

    for tx in txs {
        sqlx::query(
            r#"
            INSERT INTO transactions (
                hash, from_addr, to_addr, value_wei, gas, gas_price_wei,
                max_fee_per_gas_wei, nonce, block_number, timestamp, status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(hash) DO NOTHING;
            "#,
        )
        .bind(&tx.hash)
        .bind(&tx.from)
        .bind(&tx.to)
        .bind(&tx.value_wei)
        .bind(tx.gas)
        .bind(&tx.gas_price_wei)
        .bind(&tx.max_fee_per_gas_wei)
        .bind(tx.nonce)
        .bind(tx.block_number)
        .bind(tx.timestamp)
        .bind(&tx.status)
        .execute(&mut *txn)
        .await?;
    }

    txn.commit().await?;
    Ok(())
}

pub async fn get_recent_transactions(pool: &SqlitePool, limit: i64) -> Result<Vec<NormalizedTx>> {
    #[derive(FromRow)]
    struct TxRow {
        hash: String,
        from_addr: String,
        to_addr: Option<String>,
        value_wei: String,
        gas: i64,
        gas_price_wei: Option<String>,
        max_fee_per_gas_wei: Option<String>,
        nonce: i64,
        block_number: Option<i64>,
        timestamp: Option<i64>,
        status: Option<String>,
    }

    let rows = sqlx::query_as::<_, TxRow>(
        r#"
        SELECT hash, from_addr, to_addr, value_wei, gas, gas_price_wei,
               max_fee_per_gas_wei, nonce, block_number, timestamp, status
        FROM transactions
        ORDER BY COALESCE(timestamp, 0) DESC
        LIMIT ?1;
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| NormalizedTx {
            hash: row.hash,
            from: row.from_addr,
            to: row.to_addr,
            value_wei: row.value_wei,
            gas: row.gas,
            gas_price_wei: row.gas_price_wei,
            max_fee_per_gas_wei: row.max_fee_per_gas_wei,
            nonce: row.nonce,
            block_number: row.block_number,
            timestamp: row.timestamp,
            status: row.status,
        })
        .collect())
}

pub async fn get_top_senders(pool: &SqlitePool, limit: i64) -> Result<Vec<TopSender>> {
    #[derive(FromRow)]
    struct Row {
        address: String,
        count: i64,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT from_addr as address, COUNT(*) as count
        FROM transactions
        GROUP BY from_addr
        ORDER BY count DESC
        LIMIT ?1;
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TopSender {
            address: row.address,
            count: row.count,
        })
        .collect())
}

pub async fn get_gas_stats(pool: &SqlitePool, last_n_blocks: i64) -> Result<Option<GasStats>> {
    #[derive(FromRow)]
    struct Row {
        min_gas_price: Option<i64>,
        max_gas_price: Option<i64>,
        avg_gas_price: Option<f64>,
    }

    let row = sqlx::query_as::<_, Row>(
        // Filter to numeric strings of reasonable length before casting to avoid overflow.
        r#"
        SELECT
            MIN(CAST(gas_price_wei AS INTEGER)) as min_gas_price,
            MAX(CAST(gas_price_wei AS INTEGER)) as max_gas_price,
            AVG(CAST(gas_price_wei AS INTEGER)) as avg_gas_price
        FROM transactions
        WHERE gas_price_wei IS NOT NULL
          AND gas_price_wei GLOB '[0-9]*'
          AND LENGTH(gas_price_wei) <= 18
          AND block_number IN (
              SELECT block_number
              FROM blocks
              ORDER BY block_number DESC
              LIMIT ?1
          );
        "#,
    )
    .bind(last_n_blocks)
    .fetch_one(pool)
    .await?;

    match (row.min_gas_price, row.max_gas_price, row.avg_gas_price) {
        (Some(min), Some(max), Some(avg)) => Ok(Some(GasStats { min, max, avg })),
        _ => Ok(None),
    }
}

async fn verify_value_wei_column(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info(transactions);")
        .fetch_all(pool)
        .await?;

    let mut value_is_text = false;
    let mut gas_price_is_text = false;
    let mut max_fee_is_text = false;

    for row in rows {
        let name: String = row.try_get("name")?;
        let col_type: Option<String> = row.try_get("type")?;
        match name.as_str() {
            "value_wei" => {
                value_is_text = col_type.as_deref() == Some("TEXT");
            }
            "gas_price_wei" => {
                gas_price_is_text = col_type.as_deref() == Some("TEXT");
            }
            "max_fee_per_gas_wei" => {
                max_fee_is_text = col_type.as_deref() == Some("TEXT");
            }
            _ => {}
        }
    }

    if !value_is_text {
        warn!("transactions.value_wei is not TEXT; delete/recreate DB to pick up new schema");
    }
    if !gas_price_is_text {
        warn!("transactions.gas_price_wei is not TEXT; delete/recreate DB to pick up new schema");
    }
    if !max_fee_is_text {
        warn!("transactions.max_fee_per_gas_wei is not TEXT; delete/recreate DB to pick up new schema");
    }
    Ok(())
}
