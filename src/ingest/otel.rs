//! OTLP/JSON traces parser + ingest store.
//!
//! Slice-3 scope: traces signal only. Metrics/logs are future work.
//! The parser is intentionally permissive about unknown fields (source-preserving):
//! original span JSON is stored verbatim in `raw_event.payload`; we only extract
//! the fields the graph and UI care about.

use crate::db::{repo_observed, repo_raw, repo_runs};
use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use crate::model::meta::{PARSER_VERSION_OTEL, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, ObservedEvent, TelemetryFacet};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: Option<String>,
    pub status_code: Option<String>,
    pub status_message: Option<String>,
    pub start_unix_nano: i64,
    pub end_unix_nano: i64,
    pub attributes: Value,
    pub resource: Value,
    pub scope_name: Option<String>,
    pub scope_version: Option<String>,
    pub raw: Value,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RejectedSpan {
    pub reason: String,
    pub raw: Value,
}

#[derive(Debug, Default)]
pub struct ParseResult {
    pub spans: Vec<SpanRecord>,
    pub rejected: Vec<RejectedSpan>,
}

pub fn parse_otlp_json(body: &Value) -> ParseResult {
    let mut out = ParseResult::default();
    let Some(rs_arr) = body.get("resourceSpans").and_then(|v| v.as_array()) else {
        return out;
    };
    for rs in rs_arr {
        let resource = flatten_kv(rs.get("resource").and_then(|r| r.get("attributes")));
        let resource_session = string_from(&resource, "session.id");
        let scope_spans = rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for ss in scope_spans {
            let scope_name = ss
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let scope_version = ss
                .get("scope")
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let spans = ss
                .get("spans")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for span in spans {
                match extract_span(
                    &span,
                    &resource,
                    resource_session.as_deref(),
                    scope_name.clone(),
                    scope_version.clone(),
                ) {
                    Ok(rec) => out.spans.push(rec),
                    Err(reason) => out.rejected.push(RejectedSpan {
                        reason,
                        raw: span.clone(),
                    }),
                }
            }
        }
    }
    out
}

fn extract_span(
    span: &Value,
    resource: &Value,
    resource_session: Option<&str>,
    scope_name: Option<String>,
    scope_version: Option<String>,
) -> std::result::Result<SpanRecord, String> {
    let trace_id = span
        .get("traceId")
        .and_then(|v| v.as_str())
        .ok_or("missing traceId")?
        .to_string();
    if !is_hex_of_len(&trace_id, 32) {
        return Err("malformed traceId".into());
    }
    let span_id = span
        .get("spanId")
        .and_then(|v| v.as_str())
        .ok_or("missing spanId")?
        .to_string();
    if !is_hex_of_len(&span_id, 16) {
        return Err("malformed spanId".into());
    }
    let parent_span_id = span
        .get("parentSpanId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let name = span
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind = span
        .get("kind")
        .and_then(|v| v.as_str())
        .map(normalize_kind)
        .map(String::from);
    let status_code = span
        .get("status")
        .and_then(|s| s.get("code"))
        .and_then(|v| v.as_str())
        .map(normalize_status)
        .map(String::from);
    let status_message = span
        .get("status")
        .and_then(|s| s.get("message"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let start = parse_unix_nano(span.get("startTimeUnixNano"))?;
    let end = parse_unix_nano(span.get("endTimeUnixNano"))?;
    let attrs = flatten_kv(span.get("attributes"));
    let span_session = string_from(&attrs, "session.id");
    let session_id = span_session.or_else(|| resource_session.map(String::from));

    Ok(SpanRecord {
        trace_id,
        span_id,
        parent_span_id,
        name,
        kind,
        status_code,
        status_message,
        start_unix_nano: start,
        end_unix_nano: end,
        attributes: attrs,
        resource: resource.clone(),
        scope_name,
        scope_version,
        raw: span.clone(),
        session_id,
    })
}

fn is_hex_of_len(s: &str, n: usize) -> bool {
    s.len() == n && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn flatten_kv(attrs: Option<&Value>) -> Value {
    let mut out = serde_json::Map::new();
    let Some(arr) = attrs.and_then(|v| v.as_array()) else {
        return Value::Object(out);
    };
    for kv in arr {
        let Some(k) = kv.get("key").and_then(|v| v.as_str()) else {
            continue;
        };
        let v = kv.get("value");
        let value = match v {
            Some(o) if o.is_object() => {
                if let Some(s) = o.get("stringValue") {
                    s.clone()
                } else if let Some(b) = o.get("boolValue") {
                    b.clone()
                } else if let Some(i) = o.get("intValue") {
                    match i.as_str().and_then(|s| s.parse::<i64>().ok()) {
                        Some(n) => Value::Number(n.into()),
                        None => i.clone(),
                    }
                } else if let Some(d) = o.get("doubleValue") {
                    d.clone()
                } else if let Some(a) = o.get("arrayValue") {
                    a.clone()
                } else {
                    o.clone()
                }
            }
            _ => Value::Null,
        };
        out.insert(k.into(), value);
    }
    Value::Object(out)
}

fn string_from(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn normalize_kind(s: &str) -> &str {
    match s {
        "SPAN_KIND_INTERNAL" | "internal" => "internal",
        "SPAN_KIND_SERVER" | "server" => "server",
        "SPAN_KIND_CLIENT" | "client" => "client",
        "SPAN_KIND_PRODUCER" | "producer" => "producer",
        "SPAN_KIND_CONSUMER" | "consumer" => "consumer",
        _ => "unspecified",
    }
}

fn normalize_status(s: &str) -> &str {
    match s {
        "STATUS_CODE_OK" | "ok" => "ok",
        "STATUS_CODE_ERROR" | "error" => "error",
        _ => "unset",
    }
}

fn parse_unix_nano(v: Option<&Value>) -> std::result::Result<i64, String> {
    match v {
        Some(Value::String(s)) => s.parse::<i64>().map_err(|_| "bad unix_nano".into()),
        Some(Value::Number(n)) => n.as_i64().ok_or_else(|| "bad unix_nano".into()),
        None => Ok(0),
        _ => Err("bad unix_nano".into()),
    }
}

#[derive(Debug, Default, Serialize)]
pub struct IngestResult {
    pub accepted_spans: u64,
    pub rejected_spans: u64,
    pub duplicate_spans: u64,
    pub sessions_touched: Vec<String>,
}

pub async fn store(
    pool: &SqlitePool,
    parsed: ParseResult,
    received_at: DateTime<Utc>,
) -> Result<IngestResult> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut result = IngestResult {
        rejected_spans: parsed.rejected.len() as u64,
        ..Default::default()
    };
    let mut touched: BTreeSet<String> = BTreeSet::new();

    for span in parsed.spans {
        // Canonical JSON for hashing (sort keys so re-POST is byte-stable).
        let canonical = canonical_json(&span.raw);
        let canonical_bytes = canonical.as_bytes().to_vec();
        let payload_sha = hex::encode(Sha256::digest(&canonical_bytes));
        let source_uri = format!("otel://traces/{}/spans/{}", span.trace_id, span.span_id);
        let raw_id = gen.generate();

        let inserted = repo_raw::insert_dedup(
            pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "otel".into(),
                source_uri,
                source_line_no: 0,
                source_byte_offset: 0,
                payload_sha256: payload_sha,
                payload: canonical_bytes,
                parse_error: None,
                captured_at: received_at,
            },
        )
        .await?;
        if !inserted {
            result.duplicate_spans += 1;
            continue;
        }

        let observed_at = unix_nano_to_utc(span.start_unix_nano).unwrap_or(received_at);
        let latency_ms = if span.end_unix_nano >= span.start_unix_nano {
            Some((span.end_unix_nano - span.start_unix_nano) / 1_000_000)
        } else {
            Some(0)
        };
        let actor = match span.kind.as_deref() {
            Some("client") => Actor::Tool,
            _ => Actor::System,
        };
        let session_id = span.session_id.clone().unwrap_or_default();
        let telemetry = TelemetryFacet {
            span_name: span.name.clone(),
            span_kind: span.kind.clone(),
            status_code: span.status_code.clone(),
            status_message: span.status_message.clone(),
            start_unix_nano: span.start_unix_nano,
            end_unix_nano: span.end_unix_nano,
            attributes: span.attributes.clone(),
            resource: span.resource.clone(),
            scope_name: span.scope_name.clone(),
            scope_version: span.scope_version.clone(),
        };

        let event = ObservedEvent {
            event_id: gen.generate(),
            raw_event_id: raw_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: session_id.clone(),
            observed_at,
            actor,
            kind: EventKind::OtelSpan,
            tool_name: span
                .attributes
                .get("tool.name")
                .and_then(|v| v.as_str())
                .map(String::from),
            trace_id: Some(span.trace_id.clone()),
            span_id: Some(span.span_id.clone()),
            parent_span_id: span.parent_span_id.clone(),
            latency_ms,
            telemetry: Some(telemetry),
            payload: serde_json::json!({"raw_span": span.raw}),
            parser_version: PARSER_VERSION_OTEL.into(),
            ..Default::default()
        };
        repo_observed::insert(pool, &event).await?;

        result.accepted_spans += 1;
        if !session_id.is_empty() {
            touched.insert(session_id);
        }
    }

    // Rebuild graph for each session we touched so HTTP consumers see fresh
    // graph_node rows immediately. Without this the /v1/sessions/:id/graph
    // endpoint returns 404 for OTel-only sessions even though observed_event
    // rows are present.
    for session_id in &touched {
        crate::graph::build::rebuild_session(pool, session_id).await?;
    }

    repo_runs::finish(
        pool,
        &run_id,
        "ok",
        serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    result.sessions_touched = touched.into_iter().collect();
    Ok(result)
}

fn canonical_json(value: &Value) -> String {
    // Recursively sort object keys so byte representation is stable.
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

fn unix_nano_to_utc(nano: i64) -> Option<DateTime<Utc>> {
    if nano <= 0 {
        return None;
    }
    let secs = nano / 1_000_000_000;
    let nsec = (nano % 1_000_000_000) as u32;
    chrono::DateTime::<Utc>::from_timestamp(secs, nsec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn single_span_fixture() -> Value {
        json!({
            "resourceSpans": [{
                "resource": {"attributes": [
                    {"key": "service.name", "value": {"stringValue": "claude-code"}},
                    {"key": "session.id",   "value": {"stringValue": "sess-otel-A"}}
                ]},
                "scopeSpans": [{
                    "scope": {"name": "witmcc.test", "version": "0.1.0"},
                    "spans": [{
                        "traceId": "5b8aa5a2d2c872e8321cf37308d69df2",
                        "spanId":  "051581bf3cb55c13",
                        "name":    "tool.invoke",
                        "kind":    "SPAN_KIND_CLIENT",
                        "startTimeUnixNano": "1734567890000000000",
                        "endTimeUnixNano":   "1734567890123000000",
                        "attributes": [
                            {"key": "tool.name", "value": {"stringValue": "Bash"}}
                        ],
                        "status": {"code": "STATUS_CODE_OK"}
                    }]
                }]
            }]
        })
    }

    #[test]
    fn parses_single_span() {
        let res = parse_otlp_json(&single_span_fixture());
        assert_eq!(res.spans.len(), 1);
        assert!(res.rejected.is_empty());
        let s = &res.spans[0];
        assert_eq!(s.trace_id, "5b8aa5a2d2c872e8321cf37308d69df2");
        assert_eq!(s.span_id, "051581bf3cb55c13");
        assert_eq!(s.name, "tool.invoke");
        assert_eq!(s.kind.as_deref(), Some("client"));
        assert_eq!(s.status_code.as_deref(), Some("ok"));
        assert_eq!(s.session_id.as_deref(), Some("sess-otel-A"));
        assert_eq!(s.scope_name.as_deref(), Some("witmcc.test"));
        assert_eq!(
            s.attributes.get("tool.name").and_then(|v| v.as_str()),
            Some("Bash")
        );
        assert_eq!(s.start_unix_nano, 1_734_567_890_000_000_000);
        assert_eq!(s.end_unix_nano, 1_734_567_890_123_000_000);
    }

    #[test]
    fn rejects_missing_trace_id() {
        let mut fx = single_span_fixture();
        fx["resourceSpans"][0]["scopeSpans"][0]["spans"][0]
            .as_object_mut()
            .unwrap()
            .remove("traceId");
        let res = parse_otlp_json(&fx);
        assert!(res.spans.is_empty());
        assert_eq!(res.rejected.len(), 1);
        assert!(res.rejected[0].reason.contains("traceId"));
    }

    #[test]
    fn rejects_malformed_hex_trace_id() {
        let mut fx = single_span_fixture();
        fx["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["traceId"] =
            serde_json::Value::String("not-hex".into());
        let res = parse_otlp_json(&fx);
        assert!(res.spans.is_empty());
        assert_eq!(res.rejected.len(), 1);
        assert!(res.rejected[0].reason.contains("malformed traceId"));
    }

    #[test]
    fn span_session_attribute_overrides_resource() {
        let mut fx = single_span_fixture();
        let attrs = fx["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]
            .as_array_mut()
            .unwrap();
        attrs.push(json!({"key": "session.id", "value": {"stringValue": "from-span"}}));
        let res = parse_otlp_json(&fx);
        assert_eq!(res.spans[0].session_id.as_deref(), Some("from-span"));
    }

    #[test]
    fn missing_session_id_results_in_none() {
        let mut fx = single_span_fixture();
        fx["resourceSpans"][0]["resource"]["attributes"]
            .as_array_mut()
            .unwrap()
            .retain(|kv| kv["key"] != "session.id");
        let res = parse_otlp_json(&fx);
        assert_eq!(res.spans[0].session_id, None);
    }

    #[test]
    fn fixtures_parse_with_expected_counts() {
        let cases = &[
            ("tests/fixtures/otel/single_span.json", 1usize, 0usize),
            ("tests/fixtures/otel/parent_child.json", 2, 0),
            ("tests/fixtures/otel/multi_resource.json", 2, 0),
            ("tests/fixtures/otel/missing_session_id.json", 1, 0),
            ("tests/fixtures/otel/malformed_traceid.json", 0, 1),
        ];
        for (path, ok, rej) in cases {
            let body: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let res = parse_otlp_json(&body);
            assert_eq!(res.spans.len(), *ok, "{path} accepted");
            assert_eq!(res.rejected.len(), *rej, "{path} rejected");
        }
    }
}
