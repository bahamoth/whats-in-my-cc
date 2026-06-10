use serde_json::{json, Value};

use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use crate::ingest::transcript::{AssistantRecord, LineMeta, ParsedRecord, UserRecord};
use crate::model::meta::{PARSER_VERSION_TRANSCRIPT, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, ObservedEvent};

pub fn map_record(
    meta: &LineMeta,
    rec: &ParsedRecord,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
) -> Result<Vec<ObservedEvent>> {
    match rec {
        ParsedRecord::User(u) => Ok(map_user(meta, u, raw_event_id, gen)),
        ParsedRecord::Assistant(a) => Ok(map_assistant(meta, a, raw_event_id, gen)),
        ParsedRecord::Attachment(_) => Ok(vec![attachment_meta(meta, raw_event_id, gen, rec)]),
        ParsedRecord::SystemMsg(_) => Ok(vec![system_summary(meta, raw_event_id, gen, rec)]),
        ParsedRecord::PermissionMode(p) => Ok(vec![session_state(
            meta,
            raw_event_id,
            gen,
            &p.session_id,
            "permission_mode",
            json!({"permissionMode": p.permission_mode}),
        )]),
        ParsedRecord::LastPrompt(l) => Ok(vec![session_state(
            meta,
            raw_event_id,
            gen,
            &l.session_id,
            "last_prompt",
            json!({"leafUuid": l.leaf_uuid}),
        )]),
        ParsedRecord::FileHistorySnapshot(f) => Ok(vec![file_history(meta, raw_event_id, gen, f)]),
        // Unknown record types are preserved in raw_event but produce no ObservedEvent.
        // Returning an error here would abort the entire ingest run for benign unknown types
        // like hook_success, hook_additional_context, etc.
        ParsedRecord::Unknown(_) => Ok(vec![]),
    }
}

fn base(meta: &LineMeta, raw_event_id: &str, gen: &mut MonotonicUlidGen) -> ObservedEvent {
    let _ = meta; // reserved for future source_uri / line_no fields on ObservedEvent
    ObservedEvent {
        event_id: gen.generate(),
        raw_event_id: raw_event_id.into(),
        schema_version: SCHEMA_VERSION.into(),
        parser_version: PARSER_VERSION_TRANSCRIPT.into(),
        observed_at: chrono::Utc::now(), // overwritten by caller
        payload: Value::Null,
        ..Default::default()
    }
}

fn map_user(
    meta: &LineMeta,
    u: &UserRecord,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
) -> Vec<ObservedEvent> {
    // tool_result branch: content is array containing {type:"tool_result", tool_use_id:..}
    if let Some(arr) = u.message.get("content").and_then(|c| c.as_array()) {
        let mut out = Vec::new();
        for (ord, item) in arr.iter().enumerate() {
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                let mut e = base(meta, raw_event_id, gen);
                e.session_id = u.session_id.clone();
                e.event_uuid = Some(u.uuid.clone());
                e.parent_uuid = u.parent_uuid.clone();
                e.observed_at = u.timestamp;
                e.actor = Actor::System;
                e.kind = EventKind::ToolResult;
                e.tool_use_id = item
                    .get("tool_use_id")
                    .and_then(|x| x.as_str())
                    .map(String::from);
                e.turn_id = u.prompt_id.clone();
                e.source_tool_assistant_uuid = u.source_tool_assistant_uuid.clone();
                e.source_tool_use_id = u.source_tool_use_id.clone();
                e.is_sidechain = u.is_sidechain;
                e.is_meta = u.is_meta;
                e.cwd = u.cwd.clone();
                e.git_branch = u.git_branch.clone();
                e.user_type = u.user_type.clone();
                e.entrypoint = u.entrypoint.clone();
                e.cc_version = u.version.clone();
                e.payload = json!({"content_ordinal": ord, "tool_result": item});
                out.push(e);
            } else if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                out.push(user_message(
                    meta,
                    u,
                    raw_event_id,
                    gen,
                    json!({"content_ordinal": ord, "text": item.get("text")}),
                ));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // string content branch
    vec![user_message(
        meta,
        u,
        raw_event_id,
        gen,
        json!({"content": u.message.get("content")}),
    )]
}

fn user_message(
    meta: &LineMeta,
    u: &UserRecord,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
    payload: Value,
) -> ObservedEvent {
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = u.session_id.clone();
    e.event_uuid = Some(u.uuid.clone());
    e.parent_uuid = u.parent_uuid.clone();
    e.observed_at = u.timestamp;
    e.actor = Actor::User;
    e.kind = EventKind::UserMessage;
    e.turn_id = u.prompt_id.clone();
    e.is_sidechain = u.is_sidechain;
    e.is_meta = u.is_meta;
    e.cwd = u.cwd.clone();
    e.git_branch = u.git_branch.clone();
    e.user_type = u.user_type.clone();
    e.entrypoint = u.entrypoint.clone();
    e.cc_version = u.version.clone();
    e.payload = payload;
    e
}

fn map_assistant(
    meta: &LineMeta,
    a: &AssistantRecord,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
) -> Vec<ObservedEvent> {
    let mut out = Vec::new();
    let arr = a
        .message
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let message_id = a
        .message
        .get("id")
        .and_then(|x| x.as_str())
        .map(String::from);
    let model = a
        .message
        .get("model")
        .and_then(|x| x.as_str())
        .map(String::from);
    for (ord, item) in arr.iter().enumerate() {
        let ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let mut e = base(meta, raw_event_id, gen);
        e.session_id = a.session_id.clone();
        e.event_uuid = Some(a.uuid.clone());
        e.parent_uuid = a.parent_uuid.clone();
        e.observed_at = a.timestamp;
        e.actor = Actor::Assistant;
        e.request_id = a.request_id.clone();
        e.message_id = message_id.clone();
        e.is_sidechain = a.is_sidechain;
        e.cwd = a.cwd.clone();
        e.git_branch = a.git_branch.clone();
        e.user_type = a.user_type.clone();
        e.entrypoint = a.entrypoint.clone();
        e.cc_version = a.version.clone();
        match ty {
            "text" => {
                e.kind = EventKind::AssistantMessage;
                e.payload = json!({"content_ordinal": ord, "text": item.get("text"), "model": model});
            }
            "thinking" => {
                e.kind = EventKind::Thinking;
                e.payload = json!({"content_ordinal": ord, "thinking": item.get("thinking"), "signature": item.get("signature")});
            }
            "tool_use" => {
                e.kind = EventKind::ToolCall;
                e.tool_use_id = item.get("id").and_then(|x| x.as_str()).map(String::from);
                e.tool_name = item.get("name").and_then(|x| x.as_str()).map(String::from);
                e.payload = json!({"content_ordinal": ord, "tool_name": e.tool_name, "input": item.get("input")});
            }
            _ => {
                e.kind = EventKind::Unknown;
                e.payload = json!({"content_ordinal": ord, "raw": item});
            }
        }
        out.push(e);
    }
    out
}

fn attachment_meta(
    meta: &LineMeta,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
    rec: &ParsedRecord,
) -> ObservedEvent {
    let ParsedRecord::Attachment(a) = rec else {
        unreachable!()
    };
    let subtype = a
        .attachment
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    // hook_system_message(차단 규칙 이름+안내문)·hook_cancelled도 hook 실행의
    // 산물이므로 hook_event로 승격한다. toolUseID/hookName이 없는 subtype은
    // 해당 키가 None으로 남는다. (real fixture: disposition_v01.jsonl)
    let is_hook = matches!(
        subtype,
        "hook_success" | "hook_additional_context" | "hook_system_message" | "hook_cancelled"
    );
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = a.session_id.clone();
    e.event_uuid = Some(a.uuid.clone());
    e.parent_uuid = a.parent_uuid.clone();
    e.observed_at = a.timestamp;
    e.is_sidechain = a.is_sidechain;
    e.cwd = a.cwd.clone();
    e.git_branch = a.git_branch.clone();
    e.user_type = a.user_type.clone();
    e.entrypoint = a.entrypoint.clone();
    e.cc_version = a.version.clone();
    e.subkind = Some(subtype.into());
    if is_hook {
        e.actor = Actor::Hook;
        e.kind = EventKind::HookEvent;
        e.tool_use_id = a
            .attachment
            .get("toolUseID")
            .and_then(|x| x.as_str())
            .map(String::from);
        e.tool_name = a
            .attachment
            .get("hookName")
            .and_then(|x| x.as_str())
            .map(String::from);
    } else {
        e.actor = Actor::System;
        e.kind = EventKind::AttachmentMeta;
    }
    e.payload = a.attachment.clone();
    e
}

fn system_summary(
    meta: &LineMeta,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
    rec: &ParsedRecord,
) -> ObservedEvent {
    let ParsedRecord::SystemMsg(s) = rec else {
        unreachable!()
    };
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = s.session_id.clone();
    e.event_uuid = Some(s.uuid.clone());
    e.parent_uuid = s.parent_uuid.clone();
    e.observed_at = s.timestamp;
    e.actor = Actor::System;
    e.kind = EventKind::SystemSummary;
    e.subkind = s.subtype.clone();
    e.tool_use_id = s.tool_use_id.clone();
    e.payload = s.rest.clone();
    e
}

fn session_state(
    meta: &LineMeta,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
    session_id: &str,
    subkind: &str,
    payload: Value,
) -> ObservedEvent {
    let _ = meta;
    let mut e = base(meta, raw_event_id, gen);
    e.session_id = session_id.into();
    e.observed_at = chrono::Utc::now();
    e.actor = Actor::System;
    e.kind = EventKind::SessionState;
    e.subkind = Some(subkind.into());
    e.payload = payload;
    e
}

fn file_history(
    meta: &LineMeta,
    raw_event_id: &str,
    gen: &mut MonotonicUlidGen,
    f: &crate::ingest::transcript::FileHistorySnapshotRecord,
) -> ObservedEvent {
    let mut e = base(meta, raw_event_id, gen);
    e.observed_at = chrono::Utc::now();
    e.actor = Actor::System;
    // slice-10a — FileHistorySnapshot is no longer a top-level EventKind. It
    // rides under SessionState + subkind, mirroring the pattern used by other
    // session-meta records. Payload is unchanged so downstream consumers still
    // see {isSnapshotUpdate, snapshot}.
    e.kind = EventKind::SessionState;
    e.subkind = Some("file_history_snapshot".into());
    e.message_id = Some(f.message_id.clone());
    e.payload = json!({"isSnapshotUpdate": f.is_snapshot_update, "snapshot": f.snapshot});
    e
}
