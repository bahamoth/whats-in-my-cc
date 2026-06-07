//! Claude Code hook event ingest.
//!
//! Source-preserving: original Claude Code stdin JSON is stored verbatim in
//! `raw_event.payload`; only known fields are extracted to populate the typed
//! `HookRecord` used by the store layer.

use crate::db::{repo_observed, repo_raw, repo_runs};
use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use crate::live::{LiveEvent, LiveSink};
use crate::model::meta::{PARSER_VERSION_HOOK, SCHEMA_VERSION};
use crate::security::redaction::engine::scan;
use crate::model::observed::{Actor, EventKind, ObservedEvent};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct HookRecord {
    pub session_id: String,
    pub hook_event_name: String, // original casing, e.g. "PreToolUse"
    pub subkind: String,         // snake_case
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub cwd: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub raw: Value,
}

#[derive(Debug, Clone)]
pub struct RejectedHook {
    pub reason: String,
    pub raw: Value,
}

#[derive(Debug, Default)]
pub struct ParseResult {
    pub events: Vec<HookRecord>,
    pub rejected: Vec<RejectedHook>,
}

pub fn parse_body(body: &Value) -> ParseResult {
    let mut out = ParseResult::default();
    if let Some(arr) = body.as_array() {
        for item in arr {
            parse_one(item, &mut out);
        }
    } else if body.is_object() {
        parse_one(body, &mut out);
    } else {
        out.rejected.push(RejectedHook {
            reason: "body must be object or array".into(),
            raw: body.clone(),
        });
    }
    out
}

fn parse_one(item: &Value, out: &mut ParseResult) {
    let Some(name) = item.get("hook_event_name").and_then(|v| v.as_str()) else {
        out.rejected.push(RejectedHook {
            reason: "missing hook_event_name".into(),
            raw: item.clone(),
        });
        return;
    };
    let Some(session_id) = item
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
    else {
        out.rejected.push(RejectedHook {
            reason: "missing session_id".into(),
            raw: item.clone(),
        });
        return;
    };
    let subkind = subkind_from_name(name).to_string();
    let tool_name = item
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let tool_use_id = item
        .get("tool_use_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let cwd = item
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(String::from);
    let timestamp = item
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));

    out.events.push(HookRecord {
        session_id,
        hook_event_name: name.to_string(),
        subkind,
        tool_name,
        tool_use_id,
        cwd,
        timestamp,
        raw: item.clone(),
    });
}

#[derive(Debug, Default, Serialize)]
pub struct IngestResult {
    pub accepted_events: u64,
    pub rejected_events: u64,
    pub duplicate_events: u64,
    pub sessions_touched: Vec<String>,
}

pub async fn store(
    pool: &SqlitePool,
    parsed: ParseResult,
    received_at: DateTime<Utc>,
    sink: &dyn LiveSink,
) -> Result<IngestResult> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut result = IngestResult {
        rejected_events: parsed.rejected.len() as u64,
        ..Default::default()
    };
    let mut touched: BTreeSet<String> = BTreeSet::new();

    for ev in parsed.events {
        let canonical = canonical_json(&ev.raw);
        let canonical_bytes = canonical.as_bytes().to_vec();
        let payload_sha = hex::encode(Sha256::digest(&canonical_bytes));
        let source_uri = format!(
            "hook://{}/{}/{}",
            ev.session_id,
            ev.hook_event_name,
            ev.tool_use_id.as_deref().unwrap_or("")
        );
        let raw_id = gen.generate();

        // Slice-18: scan hook payload for secrets before storing.
        let payload_str = String::from_utf8_lossy(&canonical_bytes);
        let hook_scan = scan(&payload_str);
        let stored_bytes: Vec<u8> = if hook_scan.applied {
            hook_scan.masked_text.as_bytes().to_vec()
        } else {
            canonical_bytes
        };
        let hook_redaction_state = hook_scan.manifest.redaction_state.as_str().to_owned();
        let hook_redaction_manifest = serde_json::to_string(&hook_scan.manifest).ok();
        let inserted = repo_raw::insert_dedup(
            pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "hook".into(),
                source_uri,
                source_line_no: 0,
                source_byte_offset: 0,
                payload_sha256: payload_sha,
                payload: stored_bytes,
                parse_error: None,
                captured_at: received_at,
                redaction_state: hook_redaction_state,
                redaction_manifest: hook_redaction_manifest,
            },
        )
        .await?;

        // Self-heal (DEV-S3-07): mark session touched BEFORE the dedup check so a
        // re-POST still re-runs the insight pipeline for the session.
        touched.insert(ev.session_id.clone());

        if !inserted {
            result.duplicate_events += 1;
            continue;
        }

        let observed_at = ev.timestamp.unwrap_or(received_at);
        let event = ObservedEvent {
            event_id: gen.generate(),
            raw_event_id: raw_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: ev.session_id.clone(),
            observed_at,
            actor: Actor::Hook,
            kind: EventKind::HookEvent,
            subkind: Some(ev.subkind.clone()),
            tool_use_id: ev.tool_use_id.clone(),
            tool_name: ev.tool_name.clone(),
            cwd: ev.cwd.clone(),
            payload: serde_json::json!({"hook": ev.raw}),
            parser_version: PARSER_VERSION_HOOK.into(),
            ..Default::default()
        };
        repo_observed::insert(pool, &event).await?;
        sink.emit(LiveEvent {
            schema_version: LiveEvent::SCHEMA_VERSION.into(),
            session_id: event.session_id.clone(),
            event_id: event.event_id.clone(),
            kind: event.kind,
            source_type: "hook".into(),
            observed_at: event.observed_at.to_rfc3339(),
        });

        result.accepted_events += 1;
    }

    for session_id in &touched {
        crate::insight::pipeline::run_detectors(pool, session_id).await?;
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

fn subkind_from_name(name: &str) -> &'static str {
    match name {
        "PreToolUse" => "pre_tool_use",
        "PostToolUse" => "post_tool_use",
        "UserPromptSubmit" => "user_prompt_submit",
        "Stop" => "stop",
        "SubagentStop" => "subagent_stop",
        "Notification" => "notification",
        "PreCompact" => "pre_compact",
        "SessionStart" => "session_start",
        "SessionEnd" => "session_end",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pre_tool_use_fixture() -> serde_json::Value {
        json!({
            "session_id":      "sess_A",
            "hook_event_name": "PreToolUse",
            "tool_name":       "Bash",
            "tool_input":      {"command": "ls"},
            "tool_use_id":     "toolu_01"
        })
    }

    #[test]
    fn parses_single_object() {
        let res = parse_body(&pre_tool_use_fixture());
        assert_eq!(res.events.len(), 1);
        assert!(res.rejected.is_empty());
        let ev = &res.events[0];
        assert_eq!(ev.session_id, "sess_A");
        assert_eq!(ev.hook_event_name, "PreToolUse");
        assert_eq!(ev.subkind, "pre_tool_use");
        assert_eq!(ev.tool_name.as_deref(), Some("Bash"));
        assert_eq!(ev.tool_use_id.as_deref(), Some("toolu_01"));
    }

    #[test]
    fn parses_array_body() {
        let body = json!([
            pre_tool_use_fixture(),
            {"session_id": "sess_A", "hook_event_name": "Stop"}
        ]);
        let res = parse_body(&body);
        assert_eq!(res.events.len(), 2);
        assert_eq!(res.events[1].subkind, "stop");
    }

    #[test]
    fn rejects_missing_hook_event_name() {
        let body = json!({"session_id": "sess_A"});
        let res = parse_body(&body);
        assert!(res.events.is_empty());
        assert_eq!(res.rejected.len(), 1);
        assert!(res.rejected[0].reason.contains("hook_event_name"));
    }

    #[test]
    fn rejects_empty_session_id() {
        let body = json!({"session_id": "", "hook_event_name": "Stop"});
        let res = parse_body(&body);
        assert_eq!(res.rejected.len(), 1);
    }

    #[test]
    fn unknown_event_name_is_accepted_with_unknown_subkind() {
        let body = json!({"session_id": "sess_A", "hook_event_name": "FutureHook"});
        let res = parse_body(&body);
        assert_eq!(res.events.len(), 1);
        assert_eq!(res.events[0].subkind, "unknown");
    }

    #[test]
    fn body_must_be_object_or_array() {
        let res = parse_body(&json!("nope"));
        assert!(res.events.is_empty());
        assert_eq!(res.rejected.len(), 1);
    }

    #[tokio::test]
    async fn store_persists_event_and_dedupes_on_replay() {
        use crate::db::{migrate, repo_observed};
        use chrono::Utc;
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();

        let body = pre_tool_use_fixture();
        let parsed = parse_body(&body);
        let first = store(&pool, parsed, Utc::now(), &crate::live::NoopSink).await.unwrap();
        assert_eq!(first.accepted_events, 1);
        assert_eq!(first.duplicate_events, 0);
        assert_eq!(first.sessions_touched, vec!["sess_A".to_string()]);

        let parsed2 = parse_body(&body);
        let second = store(&pool, parsed2, Utc::now(), &crate::live::NoopSink).await.unwrap();
        assert_eq!(second.accepted_events, 0);
        assert_eq!(second.duplicate_events, 1);
        // Self-heal (DEV-S3-07): even on full duplicate, session is still touched.
        assert_eq!(second.sessions_touched, vec!["sess_A".to_string()]);

        let rows = repo_observed::list_session(&pool, "sess_A", 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert!(matches!(
            r.kind,
            crate::model::observed::EventKind::HookEvent
        ));
        assert_eq!(r.subkind.as_deref(), Some("pre_tool_use"));
        assert_eq!(r.tool_use_id.as_deref(), Some("toolu_01"));
        assert_eq!(r.tool_name.as_deref(), Some("Bash"));
        assert!(matches!(r.actor, crate::model::observed::Actor::Hook));
        assert_eq!(r.parser_version, "hook@0.1.0");
    }

    #[test]
    fn fixtures_parse_with_expected_counts() {
        let cases = &[
            ("tests/fixtures/hook/pre_tool_use.json", 1usize, 0usize),
            ("tests/fixtures/hook/post_tool_use.json", 1, 0),
            ("tests/fixtures/hook/user_prompt_submit.json", 1, 0),
            ("tests/fixtures/hook/notification.json", 1, 0),
            ("tests/fixtures/hook/pre_compact.json", 1, 0),
            ("tests/fixtures/hook/session_start.json", 1, 0),
            ("tests/fixtures/hook/session_end.json", 1, 0),
            ("tests/fixtures/hook/stop.json", 1, 0),
            ("tests/fixtures/hook/subagent_stop.json", 1, 0),
            ("tests/fixtures/hook/batch_three.json", 3, 0),
            ("tests/fixtures/hook/missing_session_id.json", 0, 1),
            ("tests/fixtures/hook/unknown_event.json", 1, 0),
        ];
        for (path, ok, rej) in cases {
            let body: Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let res = parse_body(&body);
            assert_eq!(res.events.len(), *ok, "{path} accepted");
            assert_eq!(res.rejected.len(), *rej, "{path} rejected");
        }
    }

    #[test]
    fn maps_all_nine_known_names_to_snake_case() {
        for (name, expected) in [
            ("PreToolUse", "pre_tool_use"),
            ("PostToolUse", "post_tool_use"),
            ("UserPromptSubmit", "user_prompt_submit"),
            ("Stop", "stop"),
            ("SubagentStop", "subagent_stop"),
            ("Notification", "notification"),
            ("PreCompact", "pre_compact"),
            ("SessionStart", "session_start"),
            ("SessionEnd", "session_end"),
        ] {
            let body = json!({"session_id": "s", "hook_event_name": name});
            let res = parse_body(&body);
            assert_eq!(res.events.len(), 1, "{name}");
            assert_eq!(res.events[0].subkind, expected, "{name}");
        }
    }
}
