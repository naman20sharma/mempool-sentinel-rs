use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct IngestStats {
    blocks: AtomicU64,
    transactions: AtomicU64,
    pending_transactions: AtomicU64,
}

impl Default for IngestStats {
    fn default() -> Self {
        Self::new()
    }
}

impl IngestStats {
    pub const fn new() -> Self {
        Self {
            blocks: AtomicU64::new(0),
            transactions: AtomicU64::new(0),
            pending_transactions: AtomicU64::new(0),
        }
    }

    pub fn inc_blocks(&self, n: u64) {
        self.blocks.fetch_add(n, Ordering::Relaxed);
    }

    pub fn inc_transactions(&self, n: u64) {
        self.transactions.fetch_add(n, Ordering::Relaxed);
    }

    pub fn inc_pending_transactions(&self, n: u64) {
        self.pending_transactions.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> IngestSnapshot {
        IngestSnapshot {
            blocks: self.blocks.load(Ordering::Relaxed),
            transactions: self.transactions.load(Ordering::Relaxed),
            pending_transactions: self.pending_transactions.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IngestSnapshot {
    pub blocks: u64,
    pub transactions: u64,
    pub pending_transactions: u64,
}

pub static INGEST_STATS: IngestStats = IngestStats::new();
