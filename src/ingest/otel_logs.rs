//! Slice-6 Stage 2 — normalise OTLP/JSON logs into `LogRecord` ObservedEvents.
//!
//! Anchored on `tests/fixtures/otel/real/logs_v01.json`. The real fixture
//! carries two `event.name` values from claude-code 2.1.145 —
//! `hook_execution_complete` and `mcp_server_connection`. We pull `event.name`
//! out of attributes for indexing, keep the OTLP `body` verbatim, and surface
//! `trace_id` / `span_id` on the ObservedEvent row when present so later slices
//! can build span↔log edges without re-parsing the payload.

use crate::db::repo_observed;
use crate::error::Result;
use crate::ingest::otel::unix_nano_to_utc;
use crate::ingest::otel_metrics::{flatten_attrs, unwrap_any_value};
use crate::model::meta::{PARSER_VERSION_OTEL_LOGS, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, LogFacet, ObservedEvent};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Default, Serialize)]
pub struct LogsIngestResult {
    pub accepted_log_records: u64,
    pub duplicate_log_records: u64,
    pub rejected_log_records: u64,
    pub sessions_touched: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LogRecordItem {
    pub facet: LogFacet,
    pub session_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

pub fn parse_request(body: &Value) -> Vec<LogRecordItem> {
    let mut out = Vec::new();
    let rls = match body.get("resourceLogs").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return out,
    };
    for rl in rls {
        let resource_attrs = flatten_attrs(rl.get("resource").and_then(|r| r.get("attributes")));
        let scope_logs = match rl.get("scopeLogs").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for sl in scope_logs {
            let scope_name = sl
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let scope_version = sl
                .get("scope")
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let records = match sl.get("logRecords").and_then(|v| v.as_array()) {
                Some(a) => a,
                None => continue,
            };
            for lr in records {
                let attrs = flatten_attrs(lr.get("attributes"));
                let body_v = lr.get("body").map(unwrap_any_value).unwrap_or(Value::Null);
                let event_name = attrs
                    .get("event.name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let severity_number = lr.get("severityNumber").and_then(|v| v.as_i64()).map(|n| n as i32);
                let severity_text = lr
                    .get("severityText")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let time_unix_nano = parse_unix_nano(lr.get("timeUnixNano")).unwrap_or(0);
                let observed_time_unix_nano = parse_unix_nano(lr.get("observedTimeUnixNano"));
                let trace_id = lr.get("traceId").and_then(|v| v.as_str()).map(String::from);
                let span_id = lr.get("spanId").and_then(|v| v.as_str()).map(String::from);
                let session_id = attrs
                    .get("session.id")
                    .and_then(|v| v.as_str())
                    .or_else(|| resource_attrs.get("session.id").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string();
                let facet = LogFacet {
                    severity_number,
                    severity_text,
                    body: body_v,
                    event_name,
                    attributes: attrs,
                    resource: resource_attrs.clone(),
                    scope_name: scope_name.clone(),
                    scope_version: scope_version.clone(),
                    time_unix_nano,
                    observed_time_unix_nano,
                };
                out.push(LogRecordItem {
                    facet,
                    session_id,
                    trace_id,
                    span_id,
                });
            }
        }
    }
    out
}

fn parse_unix_nano(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::String(s)) => s.parse::<i64>().ok(),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

/// `log:<resource_sha8>:<time_unix_nano>:<body_sha8>:<attr_sha8>`
fn derive_event_id(item: &LogRecordItem) -> String {
    let resource_sha = sha8(&item.facet.resource);
    let body_sha = sha8(&item.facet.body);
    let attr_sha = sha8(&item.facet.attributes);
    format!(
        "log:{}:{}:{}:{}",
        resource_sha, item.facet.time_unix_nano, body_sha, attr_sha
    )
}

fn sha8(v: &Value) -> String {
    let canon = canonical_json(v);
    let hash = hex::encode(Sha256::digest(canon.as_bytes()));
    hash[..8].to_string()
}

fn canonical_json(value: &Value) -> String {
    fn norm(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), norm(&map[k]));
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(norm).collect()),
            _ => v.clone(),
        }
    }
    norm(value).to_string()
}

fn pick_actor(event_name: Option<&str>) -> Actor {
    match event_name {
        Some(n) if n.starts_with("hook_") => Actor::Hook,
        _ => Actor::System,
    }
}

pub async fn store_request(
    pool: &SqlitePool,
    raw_event_id: &str,
    body: &Value,
    received_at: DateTime<Utc>,
) -> Result<LogsIngestResult> {
    let items = parse_request(body);
    let mut result = LogsIngestResult::default();
    let mut touched: BTreeSet<String> = BTreeSet::new();

    for it in items {
        let observed_at = unix_nano_to_utc(it.facet.time_unix_nano).unwrap_or(received_at);
        let event_id = derive_event_id(&it);
        if !it.session_id.is_empty() {
            touched.insert(it.session_id.clone());
        }
        let actor = pick_actor(it.facet.event_name.as_deref());
        let payload = serde_json::to_value(&it.facet).unwrap_or(json!({}));
        let event = ObservedEvent {
            event_id,
            raw_event_id: raw_event_id.to_string(),
            schema_version: SCHEMA_VERSION.into(),
            session_id: it.session_id.clone(),
            observed_at,
            actor,
            kind: EventKind::LogRecord,
            trace_id: it.trace_id.clone(),
            span_id: it.span_id.clone(),
            parser_version: PARSER_VERSION_OTEL_LOGS.into(),
            payload,
            ..Default::default()
        };
        let inserted = repo_observed::insert_or_ignore(pool, &event).await?;
        if inserted {
            result.accepted_log_records += 1;
        } else {
            result.duplicate_log_records += 1;
        }
    }

    for sid in &touched {
        crate::graph::build::rebuild_session(pool, sid).await?;
    }
    result.sessions_touched = touched.into_iter().collect();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_real() -> Value {
        serde_json::from_str(
            &std::fs::read_to_string("tests/fixtures/otel/real/logs_v01.json").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn parse_real_fixture_extracts_event_names() {
        let body = load_real();
        let items = parse_request(&body);
        assert!(!items.is_empty(), "real logs fixture has records");
        let names: BTreeSet<String> = items
            .iter()
            .filter_map(|i| i.facet.event_name.clone())
            .collect();
        // observed via inspect: hook_execution_complete + mcp_server_connection
        assert!(
            names.contains("hook_execution_complete") || names.contains("mcp_server_connection"),
            "expected at least one known event.name; got {names:?}"
        );
    }

    #[test]
    fn real_fixture_carries_session_id() {
        let body = load_real();
        let items = parse_request(&body);
        for i in &items {
            assert!(!i.session_id.is_empty(), "every log record has session.id");
        }
    }

    #[test]
    fn hook_event_actor_is_hook() {
        assert!(matches!(pick_actor(Some("hook_execution_complete")), Actor::Hook));
        assert!(matches!(pick_actor(Some("mcp_server_connection")), Actor::System));
        assert!(matches!(pick_actor(None), Actor::System));
    }

    #[test]
    fn event_id_is_deterministic() {
        let body = load_real();
        let id1 = derive_event_id(parse_request(&body).first().unwrap());
        let id2 = derive_event_id(parse_request(&body).first().unwrap());
        assert_eq!(id1, id2);
    }

    #[test]
    fn empty_resource_logs_yields_no_records() {
        assert!(parse_request(&json!({"resourceLogs": []})).is_empty());
        assert!(parse_request(&json!({})).is_empty());
    }
}
