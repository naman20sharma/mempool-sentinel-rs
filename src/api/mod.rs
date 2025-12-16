use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::ingest_stats::INGEST_STATS;
use crate::models::{GasStats, NormalizedTx, TopSender};
use crate::storage::{self, DbPool};

#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct TopSendersResponse {
    top_senders: Vec<TopSender>,
}

#[derive(Serialize)]
struct GasStatsResponse {
    min: Option<i64>,
    max: Option<i64>,
    avg: Option<f64>,
}

#[derive(Serialize)]
struct IngestStatsResponse {
    blocks: u64,
    transactions: u64,
    pending_transactions: u64,
}

#[derive(Serialize)]
struct RecentTxsResponse {
    transactions: Vec<NormalizedTx>,
}

pub async fn run_http_server(addr: &str, pool: DbPool) -> Result<()> {
    let state = AppState { pool };
    let app = app_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual = listener.local_addr()?;
    tracing::info!("HTTP server listening on http://{}", actual);

    axum::serve(listener, app).await?;
    Ok(())
}

pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/stats/top-senders", get(stats_top_senders))
        .route("/stats/gas", get(stats_gas))
        .route("/stats/ingest", get(stats_ingest))
        .route("/tx/recent", get(recent_txs))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Debug, Deserialize)]
struct TopSendersParams {
    limit: Option<u64>,
}

async fn stats_top_senders(
    State(state): State<AppState>,
    Query(params): Query<TopSendersParams>,
) -> Result<Json<TopSendersResponse>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(10) as i64;
    let rows = storage::get_top_senders(&state.pool, limit)
        .await
        .map_err(internal_error)?;
    Ok(Json(TopSendersResponse { top_senders: rows }))
}

#[derive(Debug, Deserialize)]
struct GasStatsParams {
    blocks: Option<u64>,
}

async fn stats_gas(
    State(state): State<AppState>,
    Query(params): Query<GasStatsParams>,
) -> Result<Json<GasStatsResponse>, (StatusCode, String)> {
    let blocks = params.blocks.unwrap_or(50) as i64;
    let stats = storage::get_gas_stats(&state.pool, blocks)
        .await
        .map_err(internal_error)?;

    let response = match stats {
        Some(GasStats { min, max, avg }) => GasStatsResponse {
            min: Some(min),
            max: Some(max),
            avg: Some(avg),
        },
        None => GasStatsResponse {
            min: None,
            max: None,
            avg: None,
        },
    };

    Ok(Json(response))
}

async fn stats_ingest() -> Json<IngestStatsResponse> {
    let snap = INGEST_STATS.snapshot();
    Json(IngestStatsResponse {
        blocks: snap.blocks,
        transactions: snap.transactions,
        pending_transactions: snap.pending_transactions,
    })
}

#[derive(Debug, Deserialize)]
struct RecentTxParams {
    limit: Option<u64>,
}

async fn recent_txs(
    State(state): State<AppState>,
    Query(params): Query<RecentTxParams>,
) -> Result<Json<RecentTxsResponse>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(20) as i64;
    let txs = storage::get_recent_transactions(&state.pool, limit)
        .await
        .map_err(internal_error)?;
    Ok(Json(RecentTxsResponse { transactions: txs }))
}

fn internal_error<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
