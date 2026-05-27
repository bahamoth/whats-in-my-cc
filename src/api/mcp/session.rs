//! Slice-17 — MCP session state.
//!
//! Each `initialize` call creates an `McpSession` stored in the in-memory
//! registry. The session id is echoed in `Mcp-Session-Id` response header and
//! required on subsequent requests.
//!
//! DEV-S17-04: sessions are in-memory only; server restart invalidates them.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use serde_json::Value;

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
}

impl McpSession {
    pub fn new(session_id: String) -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            session_id,
            initialized: false,
            notif_tx: tx,
        }
    }
}

/// Thread-safe registry of active MCP sessions.
#[derive(Clone, Default)]
pub struct SessionRegistry {
    inner: Arc<RwLock<HashMap<String, McpSession>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a new session, return its id.
    pub async fn insert(&self, session: McpSession) -> String {
        let id = session.session_id.clone();
        self.inner.write().await.insert(id.clone(), session);
        id
    }

    /// Mark a session as initialized (after notifications/initialized).
    pub async fn mark_initialized(&self, session_id: &str) {
        let mut map = self.inner.write().await;
        if let Some(s) = map.get_mut(session_id) {
            s.initialized = true;
        }
    }

    /// True if the session exists.
    pub async fn exists(&self, session_id: &str) -> bool {
        self.inner.read().await.contains_key(session_id)
    }

    /// Subscribe to notifications for a session. Returns None if session unknown.
    pub async fn subscribe(
        &self,
        session_id: &str,
    ) -> Option<broadcast::Receiver<McpNotification>> {
        let map = self.inner.read().await;
        map.get(session_id).map(|s| s.notif_tx.subscribe())
    }

    /// Send a notification to all SSE clients for the given session.
    /// Silently ignores send errors (no connected receivers).
    pub async fn notify(&self, session_id: &str, notif: McpNotification) {
        let map = self.inner.read().await;
        if let Some(s) = map.get(session_id) {
            let _ = s.notif_tx.send(notif);
        }
    }

    /// Broadcast a notification to ALL sessions (e.g., on rebuild_session).
    pub async fn broadcast_all(&self, notif: McpNotification) {
        let map = self.inner.read().await;
        for session in map.values() {
            let _ = session.notif_tx.send(notif.clone());
        }
    }
}
