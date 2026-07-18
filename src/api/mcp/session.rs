//! Slice-17 — MCP session state.
//!
//! Each `initialize` call creates an `McpSession` stored in the in-memory
//! registry. The session id is echoed in `Mcp-Session-Id` response header and
//! required on subsequent requests.
//!
//! DEV-S17-04: sessions are in-memory only; server restart invalidates them.
//!
//! growth-2026-07-18 — idle-TTL eviction: the registry previously only ever
//! grew (insert per `initialize`, no remove anywhere), so repeated client
//! reconnects accumulated sessions + broadcast channels for the process
//! lifetime. Sessions idle past [`SESSION_IDLE_TTL`] are lazily evicted on the
//! next insert; `exists`/`subscribe` (the per-request access paths) refresh
//! `last_seen`. Session termination is a normal event in the MCP Streamable
//! HTTP contract — a client that gets "unknown session" re-initializes.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};

/// Idle window before a session becomes evictable — generous versus any
/// active MCP conversation, small versus the process lifetime.
pub const SESSION_IDLE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Notification sent over the SSE channel to MCP clients.
#[derive(Debug, Clone)]
pub struct McpNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl McpNotification {
    pub fn resources_updated(uri: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: "notifications/resources/updated".into(),
            params: serde_json::json!({ "uri": uri }),
        }
    }
}

/// Per-session state.
#[derive(Debug)]
pub struct McpSession {
    pub session_id: String,
    pub initialized: bool,
    /// Broadcast sender for SSE notifications.
    pub notif_tx: broadcast::Sender<McpNotification>,
    /// Last request-path access (insert / exists / subscribe) — TTL basis.
    last_seen: Instant,
}

impl McpSession {
    pub fn new(session_id: String) -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            session_id,
            initialized: false,
            notif_tx: tx,
            last_seen: Instant::now(),
        }
    }
}

/// Thread-safe registry of active MCP sessions.
#[derive(Clone)]
pub struct SessionRegistry {
    inner: Arc<RwLock<HashMap<String, McpSession>>>,
    idle_ttl: Duration,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::with_ttl(SESSION_IDLE_TTL)
    }

    /// Test hook: registry with a custom idle TTL.
    pub fn with_ttl(idle_ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            idle_ttl,
        }
    }

    /// Insert a new session, return its id. Sessions idle past the TTL are
    /// evicted here — lazy sweep, no background task.
    pub async fn insert(&self, session: McpSession) -> String {
        let id = session.session_id.clone();
        let mut map = self.inner.write().await;
        let ttl = self.idle_ttl;
        map.retain(|_, s| s.last_seen.elapsed() < ttl);
        map.insert(id.clone(), session);
        id
    }

    /// Mark a session as initialized (after notifications/initialized).
    pub async fn mark_initialized(&self, session_id: &str) {
        let mut map = self.inner.write().await;
        if let Some(s) = map.get_mut(session_id) {
            s.initialized = true;
            s.last_seen = Instant::now();
        }
    }

    /// True if the session exists. Access refreshes the idle clock.
    pub async fn exists(&self, session_id: &str) -> bool {
        let mut map = self.inner.write().await;
        match map.get_mut(session_id) {
            Some(s) => {
                s.last_seen = Instant::now();
                true
            }
            None => false,
        }
    }

    /// Subscribe to notifications for a session. Returns None if session
    /// unknown. Access refreshes the idle clock.
    pub async fn subscribe(
        &self,
        session_id: &str,
    ) -> Option<broadcast::Receiver<McpNotification>> {
        let mut map = self.inner.write().await;
        map.get_mut(session_id).map(|s| {
            s.last_seen = Instant::now();
            s.notif_tx.subscribe()
        })
    }

    /// Send a notification to all SSE clients for the given session.
    /// Silently ignores send errors (no connected receivers).
    pub async fn notify(&self, session_id: &str, notif: McpNotification) {
        let map = self.inner.read().await;
        if let Some(s) = map.get(session_id) {
            let _ = s.notif_tx.send(notif);
        }
    }

    /// Broadcast a notification to ALL sessions (e.g., after ingest).
    pub async fn broadcast_all(&self, notif: McpNotification) {
        let map = self.inner.read().await;
        for session in map.values() {
            let _ = session.notif_tx.send(notif.clone());
        }
    }
}
