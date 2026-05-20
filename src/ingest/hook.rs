//! Claude Code hook event ingest.
//!
//! Source-preserving: original Claude Code stdin JSON is stored verbatim in
//! `raw_event.payload`; only known fields are extracted to populate the typed
//! `HookRecord` used by the store layer.

use chrono::{DateTime, Utc};
use serde_json::Value;

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
