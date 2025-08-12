# mempool-sentinel-rs
Compact Ethereum ingestion service written in Rust. Collects block data (and optional pending transactions), persists them in SQLite, provides a CLI for queries, and exposes an HTTP API for stats.

## Capabilities
- Poll recent Ethereum blocks over HTTP, normalize transactions, and store block/transaction rows in SQLite.
- Sample pending transactions over WebSocket when `ETH_WS_URL` is provided, buffering and inserting in batches.
- Query stored data via CLI commands (gas stats, top senders, recent transactions) and lightweight HTTP endpoints.
- Track ingestion counts in process memory for quick health checks.

## Architecture overview
- Tokio async runtime with `reqwest`/`ethers` for RPC access.
- SQLite via `sqlx` with two tables (`blocks`, `transactions`); schema applied inline at startup.
- CLI built with `clap`; HTTP API built with `axum`.
- Optional address filtering (`FILTER_ADDRESSES`) applied during both block ingestion and mempool sampling.
- Makefile coordinates fmt/lint/test/dev workflows.

## Requirements
- Rust 1.70+ (tested on 1.92.0).
- SQLite (bundled with `sqlx`; no external service required).
- Ethereum RPC endpoints:
  - `ETH_RPC_URL` (HTTP) is mandatory.
  - `ETH_WS_URL` (WebSocket) is required for mempool sampling.
- `.env` or environment variables should **not** contain API keys committed to source control. If a key leaks, rotate it immediately via your RPC provider.

## Quick start
```bash
git clone https://github.com/naman20sharma/mempool-sentinel-rs.git
cd rust-eth-mempool-lab
cp .env.example .env            # set ETH_RPC_URL (and ETH_WS_URL if sampling mempool)
make demo                       # ingest last 5 blocks and start the HTTP API
```
`make demo` builds the binary, runs `ingest-once --blocks 5`, then starts `serve --addr 127.0.0.1:8080`. The API remains available until you stop the process (Ctrl+C).

## Commands reference
### Make targets
- `make demo` – ingest recent blocks (default 5) and start the API server.
- `make serve` – start the API server using current DB.
- `make mempool` – run `mempool-sample` (requires `ETH_WS_URL`, duration/max tunable via `MEMPOOL_DURATION`, `MEMPOOL_MAX`).
- `make fmt` / `make lint` / `make test` – formatting, clippy, and tests.

### Direct CLI commands
```bash
cargo run -- ingest-once --blocks N
cargo run -- mempool-sample --duration-secs 30 --max 500
cargo run -- top-senders --limit 10
cargo run -- gas-stats --blocks 20
cargo run -- recent-txs --limit 20
cargo run -- serve --addr 127.0.0.1:8080
```
Environment variables (`ETH_RPC_URL`, `ETH_WS_URL`, `DATABASE_URL`, `HTTP_BIND`, `FILTER_ADDRESSES`) are read via `dotenvy`, so `.env` works out of the box.

## HTTP API endpoints
- `GET /health`
- `GET /stats/top-senders?limit=10`
- `GET /stats/gas?blocks=50`
- `GET /stats/ingest`
- `GET /tx/recent?limit=20`

## Metrics from a real run
| Metric | Value (sample run) | Note |
| --- | --- | --- |
| blocks_ingested | 5 | DB count after ingest-once (BLOCKS=5) |
| tx_ingested | 1 | DB count after ingest-once |
| pending_tx_ingested | 0 | `/stats/ingest` during run |
| insert_errors | 0 | `/stats/ingest` during run |
| mempool_received | 0 | mempool sample (30s, max=300) |
| mempool_fetched | 0 | mempool sample |
| mempool_inserted | 0 | mempool sample |
| mempool_insert_errors | 0 | mempool sample |
| ingest_duration_sec | 0.89 | timed `ingest-once --blocks 5` |
| db_size_bytes | 32K | `ls -lh ./data/mempool_lab.sqlite` |
| api_stats_gas_latency_ms | p50≈0.6ms, p95≈0.9ms | curl timing over 30 requests |

- `/stats/ingest` shows per-process counters; the ingest and serve binaries each hold their own state. If you ingest in one process and run the API in another, the API counters show zero even though SQLite already has data.
- Mempool metrics can be zero if address filtering is enabled and no matching pending transactions appear during the sampling window, or if the RPC node fails to provide full transaction details for hashes.

Tested on macOS (aarch64) with Rust 1.92.0, Alchemy mainnet RPC, `BLOCKS=5`, `MEMPOOL_DURATION=30`, `MEMPOOL_MAX=300`. Expect different numbers on other networks or providers.

## Design decisions and tradeoffs
- **HTTP block polling + optional WS sampling** keeps ingestion deterministic while still exercising WebSocket flows when needed.
- **Inline schema creation** avoids external migration tooling; warnings prompt the operator to recreate a DB if column types drift.
- **Per-process ingest counters** keep the runtime lightweight; no Prometheus dependency.
- **Batched pending inserts (100 per flush)** reduce SQLite contention.
- **`FILTER_ADDRESSES`** provides coarse filtering without additional schema overhead; it applies to both block transfers and mempool samples.

## Limitations
- No migration framework; delete and recreate the DB when schema changes.
- Gas stats cast fee strings to integers; extremely large fee values are ignored to prevent overflow.
- Pending transaction sampling depends on the RPC node returning full tx data for hashes; throughput is limited by RPC responses and filters.
- `/stats/ingest` is not persisted; restarting the server resets counters.

## Notes on reproducibility and variability
- RPC latency, block contents, filter settings, and local hardware all influence ingestion totals and API timings.
- Pending metrics are sensitive to network conditions and address filters; zeros are expected if no matching traffic is observed.
- Keep RPC credentials private. If a key appears in logs or history, rotate it immediately with your provider.

## What I learned
- SQLite column types matter: migrating `value_wei`/fee fields to TEXT avoids silent truncation.
- WebSocket pending tx subscriptions can drop hashes; fetch-and-normalize must handle missing full txs.
- Gas price casting needs guardrails to keep aggregation queries safe without losing valuable data.
