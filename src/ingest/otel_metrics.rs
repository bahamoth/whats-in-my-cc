//! Slice-6 Stage 2 — normalise OTLP/JSON metrics into `MetricSample`
//! ObservedEvents.
//!
//! One ObservedEvent per data point. `event_id` is deterministic so re-running
//! Stage 2 (e.g. on a Stage-1 raw row that arrived earlier) is idempotent.
//!
//! Anchored on `tests/fixtures/otel/real/metrics_v01.json` (claude-code
//! 2.1.145 emitted three counter instruments — `claude_code.cost.usage`,
//! `claude_code.token.usage`, `claude_code.active_time.total`). The parser is
//! permissive about unknown fields: every original byte stays in
//! `raw_event.payload`; this layer only extracts what the graph / UI need.

use crate::db::repo_observed;
use crate::error::Result;
use crate::ingest::otel::unix_nano_to_utc;
use crate::live::{LiveEvent, LiveSink};
use crate::model::meta::{PARSER_VERSION_OTEL_METRICS, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, MetricFacet, ObservedEvent};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Default, Serialize)]
pub struct MetricsIngestResult {
    pub accepted_data_points: u64,
    pub duplicate_data_points: u64,
    pub rejected_data_points: u64,
    pub sessions_touched: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MetricSampleRecord {
    pub facet: MetricFacet,
    pub session_id: String,
    pub tool_use_id: Option<String>,
    pub request_id: Option<String>,
}

/// Parse an OTLP/JSON `ExportMetricsServiceRequest` into a flat list of
/// `MetricSampleRecord` — one record per (resource × scope × metric × dataPoint).
pub fn parse_request(body: &Value) -> Vec<MetricSampleRecord> {
    let mut out = Vec::new();
    let rms = match body.get("resourceMetrics").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return out,
    };
    for rm in rms {
        let resource_attrs = flatten_attrs(rm.get("resource").and_then(|r| r.get("attributes")));
        let scope_metrics = match rm.get("scopeMetrics").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for sm in scope_metrics {
            let scope_name = sm
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let scope_version = sm
                .get("scope")
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let metrics = match sm.get("metrics").and_then(|v| v.as_array()) {
                Some(a) => a,
                None => continue,
            };
            for m in metrics {
                let name = m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let unit = m
                    .get("unit")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let description = m
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                push_for_instrument(
                    m,
                    &name,
                    unit.as_deref(),
                    description.as_deref(),
                    &resource_attrs,
                    scope_name.as_deref(),
                    scope_version.as_deref(),
                    &mut out,
                );
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn push_for_instrument(
    metric: &Value,
    name: &str,
    unit: Option<&str>,
    description: Option<&str>,
    resource: &Value,
    scope_name: Option<&str>,
    scope_version: Option<&str>,
    out: &mut Vec<MetricSampleRecord>,
) {
    // OTLP metric is exactly one of these five.
    let (kind_str, payload, temporality, is_monotonic) = if let Some(s) = metric.get("sum") {
        (
            "sum",
            s,
            temporality_str(s.get("aggregationTemporality")),
            s.get("isMonotonic").and_then(|v| v.as_bool()),
        )
    } else if let Some(g) = metric.get("gauge") {
        ("gauge", g, None, None)
    } else if let Some(h) = metric.get("histogram") {
        (
            "histogram",
            h,
            temporality_str(h.get("aggregationTemporality")),
            None,
        )
    } else if let Some(h) = metric.get("exponentialHistogram") {
        (
            "exponential_histogram",
            h,
            temporality_str(h.get("aggregationTemporality")),
            None,
        )
    } else if let Some(s) = metric.get("summary") {
        ("summary", s, None, None)
    } else {
        return;
    };

    let data_points = match payload.get("dataPoints").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return,
    };
    for dp in data_points {
        let attrs = flatten_attrs(dp.get("attributes"));
        let time_unix_nano = parse_unix_nano(dp.get("timeUnixNano")).unwrap_or(0);
        let start_time_unix_nano = parse_unix_nano(dp.get("startTimeUnixNano"));
        let value_int = dp
            .get("asInt")
            .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()));
        let value_float = dp.get("asDouble").and_then(|v| v.as_f64());
        let histogram = if matches!(kind_str, "histogram" | "exponential_histogram" | "summary") {
            Some(dp.clone())
        } else {
            None
        };
        // session_id lives in dataPoint attributes for claude-code; fall back to resource.
        let session_id = attrs
            .get("session.id")
            .and_then(|v| v.as_str())
            .or_else(|| resource.get("session.id").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        // Promote OTel attribute correlation keys to indexed columns.
        let tool_use_id = attrs
            .get("tool_use_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let request_id = attrs
            .get("request_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let facet = MetricFacet {
            instrument_name: name.to_string(),
            instrument_kind: kind_str.to_string(),
            unit: unit.map(String::from),
            description: description.map(String::from),
            temporality: temporality.clone(),
            is_monotonic,
            value_int,
            value_float,
            histogram,
            attributes: attrs,
            resource: resource.clone(),
            scope_name: scope_name.map(String::from),
            scope_version: scope_version.map(String::from),
            time_unix_nano,
            start_time_unix_nano,
        };
        out.push(MetricSampleRecord { facet, session_id, tool_use_id, request_id });
    }
}

fn temporality_str(v: Option<&Value>) -> Option<String> {
    // OTLP enum: 0 unspecified, 1 delta, 2 cumulative
    match v.and_then(|v| v.as_i64()) {
        Some(1) => Some("delta".into()),
        Some(2) => Some("cumulative".into()),
        _ => None,
    }
}

fn parse_unix_nano(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::String(s)) => s.parse::<i64>().ok(),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

/// Flatten an OTLP attribute array `[{key, value:{stringValue|intValue|...}}]`
/// into a sorted JSON object `{"key": <unwrapped>, ...}`.
pub(crate) fn flatten_attrs(arr: Option<&Value>) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(items) = arr.and_then(|v| v.as_array()) {
        for kv in items {
            let key = match kv.get("key").and_then(|v| v.as_str()) {
                Some(k) => k,
                None => continue,
            };
            let value = kv.get("value").map(unwrap_any_value).unwrap_or(Value::Null);
            map.insert(key.to_string(), value);
        }
    }
    Value::Object(map)
}

/// Unwrap an OTLP AnyValue object into a plain JSON value.
pub(crate) fn unwrap_any_value(v: &Value) -> Value {
    let obj = match v.as_object() {
        Some(o) => o,
        None => return v.clone(),
    };
    if let Some(s) = obj.get("stringValue") {
        return s.clone();
    }
    if let Some(b) = obj.get("boolValue") {
        return b.clone();
    }
    if let Some(d) = obj.get("doubleValue") {
        return d.clone();
    }
    if let Some(bytes) = obj.get("bytesValue") {
        return bytes.clone();
    }
    if let Some(iv) = obj.get("intValue") {
        // OTLP/JSON ints are strings to preserve precision; SDKs may also emit numbers.
        if let Some(s) = iv.as_str() {
            if let Ok(n) = s.parse::<i64>() {
                return Value::Number(n.into());
            }
        }
        if let Some(n) = iv.as_i64() {
            return Value::Number(n.into());
        }
    }
    if let Some(arr) = obj
        .get("arrayValue")
        .and_then(|x| x.get("values"))
        .and_then(|x| x.as_array())
    {
        return Value::Array(arr.iter().map(unwrap_any_value).collect());
    }
    if let Some(kvs) = obj
        .get("kvlistValue")
        .and_then(|x| x.get("values"))
        .and_then(|x| x.as_array())
    {
        let mut m = serde_json::Map::new();
        for kv in kvs {
            if let (Some(k), Some(val)) =
                (kv.get("key").and_then(|x| x.as_str()), kv.get("value"))
            {
                m.insert(k.to_string(), unwrap_any_value(val));
            }
        }
        return Value::Object(m);
    }
    v.clone()
}

/// `metric:<resource_sha8>:<instrument>:<time_unix_nano>:<attr_sha8>`
fn derive_event_id(rec: &MetricSampleRecord) -> String {
    let resource_sha = sha8(&rec.facet.resource);
    let attr_sha = sha8(&rec.facet.attributes);
    format!(
        "metric:{}:{}:{}:{}",
        resource_sha, rec.facet.instrument_name, rec.facet.time_unix_nano, attr_sha
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

/// Persist parsed `MetricSample` records and rebuild touched sessions' graphs.
pub async fn store_request(
    pool: &SqlitePool,
    raw_event_id: &str,
    body: &Value,
    received_at: DateTime<Utc>,
    sink: &dyn LiveSink,
) -> Result<MetricsIngestResult> {
    let records = parse_request(body);
    let mut result = MetricsIngestResult::default();
    let mut touched: BTreeSet<String> = BTreeSet::new();

    for rec in records {
        let observed_at = unix_nano_to_utc(rec.facet.time_unix_nano).unwrap_or(received_at);
        let event_id = derive_event_id(&rec);
        if !rec.session_id.is_empty() {
            touched.insert(rec.session_id.clone());
        }
        let payload = serde_json::to_value(&rec.facet).unwrap_or(json!({}));
        let event = ObservedEvent {
            event_id,
            raw_event_id: raw_event_id.to_string(),
            schema_version: SCHEMA_VERSION.into(),
            session_id: rec.session_id.clone(),
            observed_at,
            actor: Actor::System,
            kind: EventKind::MetricSample,
            tool_use_id: rec.tool_use_id.clone(),
            request_id: rec.request_id.clone(),
            parser_version: PARSER_VERSION_OTEL_METRICS.into(),
            payload,
            ..Default::default()
        };
        let inserted = repo_observed::insert_or_ignore(pool, &event).await?;
        if inserted {
            result.accepted_data_points += 1;
            sink.emit(LiveEvent {
                schema_version: LiveEvent::SCHEMA_VERSION.into(),
                session_id: event.session_id.clone(),
                event_id: event.event_id.clone(),
                kind: event.kind,
                source_type: "otel-metrics".into(),
                observed_at: event.observed_at.to_rfc3339(),
            });
        } else {
            result.duplicate_data_points += 1;
        }
    }

    for sid in &touched {
        crate::insight::pipeline::run_extractors(pool, sid).await?;
    }
    result.sessions_touched = touched.into_iter().collect();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load_real() -> Value {
        serde_json::from_str(
            &std::fs::read_to_string("tests/fixtures/otel/real/metrics_v01.json").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn parse_real_fixture_returns_at_least_three_sum_records() {
        let body = load_real();
        let recs = parse_request(&body);
        assert!(!recs.is_empty(), "real metrics fixture has data points");
        // Every Claude Code metric in the real fixture is a sum (counter).
        for r in &recs {
            assert_eq!(r.facet.instrument_kind, "sum", "instrument {} → {}", r.facet.instrument_name, r.facet.instrument_kind);
        }
        let names: BTreeSet<String> = recs.iter().map(|r| r.facet.instrument_name.clone()).collect();
        assert!(names.contains("claude_code.cost.usage"));
    }

    #[test]
    fn real_fixture_carries_session_id_per_data_point() {
        let body = load_real();
        let recs = parse_request(&body);
        for r in &recs {
            assert!(!r.session_id.is_empty(), "every data point in the real fixture has session.id");
        }
    }

    #[test]
    fn cost_usage_data_point_has_float_value() {
        let body = load_real();
        let cost = parse_request(&body)
            .into_iter()
            .find(|r| r.facet.instrument_name == "claude_code.cost.usage")
            .expect("cost.usage instrument present");
        assert!(cost.facet.value_float.is_some(), "cost.usage uses asDouble");
        assert!(cost.facet.is_monotonic == Some(true), "cost.usage is monotonic");
    }

    #[test]
    fn event_id_is_deterministic_across_calls() {
        let body = load_real();
        let id1 = derive_event_id(parse_request(&body).first().unwrap());
        let id2 = derive_event_id(parse_request(&body).first().unwrap());
        assert_eq!(id1, id2);
    }

    #[test]
    fn flatten_attrs_unwraps_string_int_bool() {
        let arr = json!([
            {"key":"s","value":{"stringValue":"hi"}},
            {"key":"i","value":{"intValue":"42"}},
            {"key":"b","value":{"boolValue":true}}
        ]);
        let flat = flatten_attrs(Some(&arr));
        assert_eq!(flat["s"], json!("hi"));
        assert_eq!(flat["i"], json!(42));
        assert_eq!(flat["b"], json!(true));
    }

    #[test]
    fn empty_resource_metrics_yields_no_records() {
        assert!(parse_request(&json!({"resourceMetrics": []})).is_empty());
        assert!(parse_request(&json!({})).is_empty());
    }
}
