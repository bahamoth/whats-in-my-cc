//! In-memory atomic counters for /v1/health.insight.* (slice-15).
//!
//! Per DEV-S15-03: counters are in-memory only, resetting on server restart.
//! Persistence is post-MVP.

use std::sync::atomic::{AtomicI64, Ordering};

/// Shared, cheaply-cloneable (via Arc) metrics bag.
#[derive(Default)]
pub struct JudgeMetrics {
    pub calls_24h: AtomicI64,
    pub cache_hits_24h: AtomicI64,
    pub cache_misses_24h: AtomicI64,
    pub budget_exhaustions_24h: AtomicI64,
}

impl JudgeMetrics {
    pub fn call(&self) {
        self.calls_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn cache_hit(&self) {
        self.cache_hits_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn cache_miss(&self) {
        self.cache_misses_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn budget_exhaustion(&self) {
        self.budget_exhaustions_24h.fetch_add(1, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            calls_24h: self.calls_24h.load(Ordering::Relaxed),
            cache_hits_24h: self.cache_hits_24h.load(Ordering::Relaxed),
            cache_misses_24h: self.cache_misses_24h.load(Ordering::Relaxed),
            budget_exhaustions_24h: self.budget_exhaustions_24h.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time snapshot of the metrics for serialisation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub calls_24h: i64,
    pub cache_hits_24h: i64,
    pub cache_misses_24h: i64,
    pub budget_exhaustions_24h: i64,
}
