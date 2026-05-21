//! Live event envelope and sinks for slice-8 SSE streaming.
//!
//! Spec: `docs/superpowers/specs/2026-05-21-witmcc-slice8-sse-design.md` §4.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::model::observed::EventKind;

/// Wire-format envelope emitted to SSE subscribers and (M6) MCP Streamable HTTP clients.
///
/// Frozen as `schema_version = "1"`. Additive optional fields are allowed without a
/// version bump (clients ignore unknown fields). Breaking changes require a new endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveEvent {
    pub schema_version: String,
    pub session_id: String,
    pub event_id: String,
    pub kind: EventKind,
    pub source_type: String,
    pub observed_at: String,
}

impl LiveEvent {
    pub const SCHEMA_VERSION: &'static str = "1";
}

/// Sink for `LiveEvent`s emitted by ingest writers.
///
/// Implementations:
/// - `NoopSink` — CLI `ingest --all` and tests that do not exercise live emission.
/// - `BroadcastSink` — production `witmcc serve` path, wraps `tokio::sync::broadcast::Sender`.
/// - `CapturingSink` — test helper, collects envelopes into a `Vec`.
///
/// `emit` is called inside the success path of `sqlx` commits and must not perform I/O.
pub trait LiveSink: Send + Sync {
    fn emit(&self, event: LiveEvent);
}

pub struct NoopSink;

impl LiveSink for NoopSink {
    fn emit(&self, _event: LiveEvent) {}
}

#[derive(Default, Clone)]
pub struct CapturingSink {
    inner: Arc<Mutex<Vec<LiveEvent>>>,
}

impl CapturingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn collected(&self) -> Vec<LiveEvent> {
        self.inner.lock().expect("CapturingSink mutex").clone()
    }
}

impl LiveSink for CapturingSink {
    fn emit(&self, event: LiveEvent) {
        self.inner.lock().expect("CapturingSink mutex").push(event);
    }
}

#[derive(Clone)]
pub struct BroadcastSink {
    tx: Arc<broadcast::Sender<LiveEvent>>,
}

impl BroadcastSink {
    pub fn new(tx: Arc<broadcast::Sender<LiveEvent>>) -> Self {
        Self { tx }
    }

    pub fn sender(&self) -> Arc<broadcast::Sender<LiveEvent>> {
        self.tx.clone()
    }
}

impl LiveSink for BroadcastSink {
    fn emit(&self, event: LiveEvent) {
        // send returns Err when there are zero receivers. That is not a bug —
        // subscribers attach lazily and ingest must never fail because no one is listening.
        let _ = self.tx.send(event);
    }
}
