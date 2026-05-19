//! OTLP/JSON traces parser + ingest store.
//!
//! Slice-3 scope: traces signal only. Metrics/logs are future work.
//! The parser is intentionally permissive about unknown fields (source-preserving):
//! original span JSON is stored verbatim in `raw_event.payload`; we only extract
//! the fields the graph and UI care about.

use serde_json::Value;

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
) -> Result<SpanRecord, String> {
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

fn parse_unix_nano(v: Option<&Value>) -> Result<i64, String> {
    match v {
        Some(Value::String(s)) => s.parse::<i64>().map_err(|_| "bad unix_nano".into()),
        Some(Value::Number(n)) => n.as_i64().ok_or_else(|| "bad unix_nano".into()),
        None => Ok(0),
        _ => Err("bad unix_nano".into()),
    }
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
}
