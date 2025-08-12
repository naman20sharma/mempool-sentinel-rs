SHELL := /bin/sh

DATABASE_URL ?= sqlite://./data/mempool_lab.sqlite
HTTP_BIND ?= 127.0.0.1:8080
BLOCKS ?= 5
MEMPOOL_DURATION ?= 30
MEMPOOL_MAX ?= 1000

.PHONY: env_check demo serve mempool fmt lint test

env_check:
	@if [ -f .env ]; then set -a; . ./.env; set +a; fi; \
	if [ -z "$$ETH_RPC_URL" ]; then \
		echo "ETH_RPC_URL is required. Put it in .env or export it in your shell."; \
		exit 1; \
	fi

demo: env_check
	@mkdir -p data
	@cargo build
	@echo "Ingesting last $(BLOCKS) blocks into $(DATABASE_URL)..."
	@DATABASE_URL=$(DATABASE_URL) ./target/debug/rust-eth-mempool-lab ingest-once --blocks $(BLOCKS)
	@echo "Starting API server on $(HTTP_BIND)..."
	@DATABASE_URL=$(DATABASE_URL) ./target/debug/rust-eth-mempool-lab serve --addr $(HTTP_BIND)

serve:
	@mkdir -p data
	@DATABASE_URL=$(DATABASE_URL) cargo run -- serve --addr $(HTTP_BIND)

mempool: env_check
	@if [ -z "$$ETH_WS_URL" ]; then \
		echo "ETH_WS_URL is required for mempool sampling"; \
		exit 1; \
	fi
	@mkdir -p data
	@DATABASE_URL=$(DATABASE_URL) cargo run -- mempool-sample --duration-secs $(MEMPOOL_DURATION) --max $(MEMPOOL_MAX)

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets --all-features

test:
	cargo test
