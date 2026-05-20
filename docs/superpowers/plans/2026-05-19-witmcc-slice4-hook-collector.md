# Slice-4 Hook Collector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Commit messages:** Do **not** add `Co-Authored-By: Claude...` (or any other Claude attribution) footers. The repository's pre-commit hook rejects commits containing them.

**Goal:** Receive Claude Code hook events directly over HTTP and persist them as `RawEvent` + `ObservedEvent`, so the timeline carries live hook lifecycle beats (PreToolUse / PostToolUse / UserPromptSubmit / Notification / PreCompact / SessionStart / SessionEnd / Stop / SubagentStop) alongside transcript and OTel.

**Architecture:** New HTTP endpoint `POST /hooks/v1/events` accepts a single Claude Code hook JSON object or an array of them, persists each as one `raw_event` + one `observed_event` (kind = `HookEvent`, subkind = snake_case hook name, source_type = `"hook"`), and rebuilds the affected sessions' graphs. Reuses the slice-3 self-heal pattern so re-POST after a binary upgrade recovers stale state. Idempotency uses canonical-JSON `payload_sha256` plus a deterministic `source_uri` derived from `(session_id, hook_event_name, tool_use_id)`.

**Tech Stack:** Rust 1.88, axum 0.7, sqlx 0.8 (SQLite), tower-http (existing decompression layer reused). Webui: React 18, TypeScript 5, Vite 5, vitest 2.

**Spec:** `docs/superpowers/specs/2026-05-19-witmcc-slice4-hook-collector-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `src/model/meta.rs` | modify | `SCHEMA_VERSION` 0.2.0 → 0.3.0; add `PARSER_VERSION_HOOK`. |
| `src/ingest/hook.rs` | create | Hook JSON parser (`parse_body` accepting object **or** array) + ingest store with self-heal rebuild. |
| `src/ingest/mod.rs` | modify | Re-export `hook` module. |
| `src/api/hook.rs` | create | `POST /hooks/v1/events` axum handler with body-size + JSON guards. |
| `src/api/mod.rs` | modify | Register `/hooks/v1/events` route. |
| `src/api/dto.rs` | modify | `HookIngestResponse` DTO (mirrors `OtelIngestResponse`). |
| `src/graph/build.rs` | modify | `HookEvent` branch: external hook records (parser_version starts with `hook_parser`) use `(session_id, hook_event_name, tool_use_id)` merge_keys; transcript-internal hooks keep current `event_uuid` keys. |
| `tests/fixtures/hook/pre_tool_use.json` | create | PreToolUse for Bash with `tool_use_id`. |
| `tests/fixtures/hook/post_tool_use.json` | create | matching PostToolUse with `tool_response`. |
| `tests/fixtures/hook/user_prompt_submit.json` | create | UserPromptSubmit with prompt text. |
| `tests/fixtures/hook/notification.json` | create | Notification with `message`. |
| `tests/fixtures/hook/pre_compact.json` | create | PreCompact with `trigger=auto`. |
| `tests/fixtures/hook/session_start.json` | create | SessionStart with `source=startup`. |
| `tests/fixtures/hook/session_end.json` | create | SessionEnd (no extra fields). |
| `tests/fixtures/hook/stop.json` | create | Stop hook. |
| `tests/fixtures/hook/subagent_stop.json` | create | SubagentStop hook. |
| `tests/fixtures/hook/batch_three.json` | create | Array of three hook events for the same session. |
| `tests/fixtures/hook/missing_session_id.json` | create | invalid: PreToolUse with empty `session_id`. |
| `tests/fixtures/hook/unknown_event.json` | create | `hook_event_name="FutureHook"`. |
| `tests/hook_ingest.rs` | create | End-to-end ingest tests via axum-test (single/batch/dedup/reject/unknown). |
| `tests/api.rs` | modify | `meta.schema_version` assertion 0.2.0 → 0.3.0. |
| `webui/src/api/laneMapping.ts` | modify | Add `'Hook'` to `LANES`; map `'hook_event'` → `'Hook'`. |
| `webui/src/components/SourcePanel.tsx` | modify | `record_type === 'hook_event'` branch with hook header + subkind-specific section + raw JSON. |
| `webui/src/components/__tests__/SourcePanel.test.tsx` | modify | Hook record rendering tests (pre_tool_use, notification, user_prompt_submit). |
| `webui/src/components/__tests__/Timeline.test.tsx` | modify | Hook lane regression: timeline renders 7 lanes; hook_event marker placed on Hook lane. |
| `webui/src/api/__tests__/laneMapping.test.ts` | modify (or create) | `laneForNodeKind('hook_event') === 'Hook'`; `LANES.length === 7`. |
| `README.md` | modify | New "Hook collector" section with forward script + settings.json sample + degrade notice. |
| `docs/implementation-notes.html` | modify | Slice-4 Overview / Intentional Deviations / Commit Reference; update `localnav`. |

---

## Branching

Work happens on `slice4-hook-collector` branched from `main` (post slice-3 merge). Create at the start of Task 1.

```bash
git checkout main && git pull --ff-only
git checkout -b slice4-hook-collector
```

---

## Task 1: Bump `SCHEMA_VERSION` + add hook parser version

**Files:**
- Modify: `src/model/meta.rs`
- Modify: `tests/api.rs` (assertion that pins the schema version)

- [ ] **Step 1: Tighten the version assertion**

Find the existing assertion in `tests/api.rs` that checks `meta.schema_version == "0.2.0"` and change it to `"0.3.0"`.

- [ ] **Step 2: Confirm cargo test fails**

```bash
cargo test --test api -- meta_schema_version
```
Expected: FAIL — `left: "0.2.0"`, `right: "0.3.0"` (or the equivalent assertion).

- [ ] **Step 3: Bump constants**

Edit `src/model/meta.rs`:

```rust
pub const SCHEMA_VERSION: &str = "0.3.0";
pub const PARSER_VERSION_TRANSCRIPT: &str = "transcript@0.1.0";
pub const PARSER_VERSION_OTEL: &str = "otel@0.1.0";
pub const PARSER_VERSION_HOOK: &str = "hook@0.1.0";
pub const COLLECTION_PROFILE: &str = "local_transcript_slice1";
```

- [ ] **Step 4: Run cargo test, confirm pass**

```bash
cargo test --test api
```
Expected: PASS (no other tests pin the version).

- [ ] **Step 5: Commit**

```bash
git add src/model/meta.rs tests/api.rs
git commit -m "chore(meta): bump SCHEMA_VERSION 0.2.0 -> 0.3.0; add PARSER_VERSION_HOOK"
```

---

## Task 2: Hook parser — single object + array shapes

**Files:**
- Create: `src/ingest/hook.rs`
- Modify: `src/ingest/mod.rs` (re-export)

- [ ] **Step 1: Write failing unit tests**

Create `src/ingest/hook.rs` with the module skeleton + `#[cfg(test)] mod tests` covering:

```rust
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
            ("PreToolUse",       "pre_tool_use"),
            ("PostToolUse",      "post_tool_use"),
            ("UserPromptSubmit", "user_prompt_submit"),
            ("Stop",             "stop"),
            ("SubagentStop",     "subagent_stop"),
            ("Notification",     "notification"),
            ("PreCompact",       "pre_compact"),
            ("SessionStart",     "session_start"),
            ("SessionEnd",       "session_end"),
        ] {
            let body = json!({"session_id": "s", "hook_event_name": name});
            let res = parse_body(&body);
            assert_eq!(res.events.len(), 1, "{name}");
            assert_eq!(res.events[0].subkind, expected, "{name}");
        }
    }
}
```

Add `pub mod hook;` to `src/ingest/mod.rs` so the test module is compiled.

- [ ] **Step 2: Run cargo test, confirm fail**

```bash
cargo test --lib ingest::hook -- --nocapture
```
Expected: FAIL — symbols `parse_body`, `ParseResult`, `HookRecord` not in scope (or compilation error).

- [ ] **Step 3: Implement parser**

Skeleton (`src/ingest/hook.rs`):

```rust
//! Claude Code hook event ingest.
//!
//! Source-preserving: original Claude Code stdin JSON is stored verbatim in
//! `raw_event.payload`; only known fields are extracted to populate the typed
//! `HookRecord` used by the store layer.

use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct HookRecord {
    pub session_id:      String,
    pub hook_event_name: String,   // original casing, e.g. "PreToolUse"
    pub subkind:         String,   // snake_case
    pub tool_name:       Option<String>,
    pub tool_use_id:     Option<String>,
    pub cwd:             Option<String>,
    pub timestamp:       Option<DateTime<Utc>>,
    pub raw:             Value,
}

#[derive(Debug, Clone)]
pub struct RejectedHook {
    pub reason: String,
    pub raw:    Value,
}

#[derive(Debug, Default)]
pub struct ParseResult {
    pub events:   Vec<HookRecord>,
    pub rejected: Vec<RejectedHook>,
}

pub fn parse_body(body: &Value) -> ParseResult {
    let mut out = ParseResult::default();
    if body.is_array() {
        for item in body.as_array().unwrap() {
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
    let tool_name = item.get("tool_name").and_then(|v| v.as_str()).map(String::from);
    let tool_use_id = item.get("tool_use_id").and_then(|v| v.as_str()).map(String::from);
    let cwd = item.get("cwd").and_then(|v| v.as_str()).map(String::from);
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
```

- [ ] **Step 4: Run cargo test, confirm pass**

```bash
cargo test --lib ingest::hook
```

- [ ] **Step 5: Commit**

```bash
git add src/ingest/hook.rs src/ingest/mod.rs
git commit -m "feat(ingest): hook parser — single + array body, 9 known event names"
```

---

## Task 3: Hook ingest store — RawEvent + ObservedEvent + graph rebuild

**Files:**
- Modify: `src/ingest/hook.rs` (add `store` + canonicalisation + tests)

- [ ] **Step 1: Failing integration-style test (inside the module)**

Append to `src/ingest/hook.rs` tests:

```rust
#[tokio::test]
async fn store_persists_event_and_dedupes_on_replay() {
    use sqlx::sqlite::SqlitePoolOptions;
    use crate::db::migrate;
    use chrono::Utc;
    use serde_json::json;

    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let body = json!({
        "session_id":      "sess_X",
        "hook_event_name": "PreToolUse",
        "tool_name":       "Bash",
        "tool_input":      {"command": "ls"},
        "tool_use_id":     "toolu_01"
    });
    let parsed = parse_body(&body);

    let first = store(&pool, parsed, Utc::now()).await.unwrap();
    assert_eq!(first.accepted_events, 1);
    assert_eq!(first.duplicate_events, 0);
    assert_eq!(first.sessions_touched, vec!["sess_X".to_string()]);

    let parsed2 = parse_body(&body);
    let second = store(&pool, parsed2, Utc::now()).await.unwrap();
    assert_eq!(second.accepted_events, 0);
    assert_eq!(second.duplicate_events, 1);
    // Self-heal: even on duplicate raw, session is still rebuilt.
    assert_eq!(second.sessions_touched, vec!["sess_X".to_string()]);

    // observed_event row exists exactly once.
    use crate::db::repo_observed;
    let rows = repo_observed::list_session(&pool, "sess_X", 100).await.unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert!(matches!(r.kind, crate::model::observed::EventKind::HookEvent));
    assert_eq!(r.subkind.as_deref(), Some("pre_tool_use"));
    assert_eq!(r.tool_use_id.as_deref(), Some("toolu_01"));
    assert_eq!(r.tool_name.as_deref(), Some("Bash"));
}
```

- [ ] **Step 2: Run, confirm fail**

```bash
cargo test --lib ingest::hook::tests::store_persists_event_and_dedupes_on_replay
```
Expected: FAIL — `store` not defined.

- [ ] **Step 3: Implement `store`**

Add to `src/ingest/hook.rs`:

```rust
use crate::db::{repo_observed, repo_raw, repo_runs};
use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use crate::model::meta::{PARSER_VERSION_HOOK, SCHEMA_VERSION};
use crate::model::observed::{Actor, EventKind, ObservedEvent};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

#[derive(Debug, Default, Serialize)]
pub struct IngestResult {
    pub accepted_events:  u64,
    pub rejected_events:  u64,
    pub duplicate_events: u64,
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
                payload: canonical_bytes,
                parse_error: None,
                captured_at: received_at,
            },
        )
        .await?;

        // Self-heal (DEV-S3-07): mark session touched BEFORE the dedup check so a
        // re-POST after stale graph state still triggers rebuild.
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

        result.accepted_events += 1;
    }

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
    fn norm(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys { out.insert(k.clone(), norm(&map[k])); }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(norm).collect()),
            _ => v.clone(),
        }
    }
    norm(value).to_string()
}
```

- [ ] **Step 4: Run, confirm pass**

```bash
cargo test --lib ingest::hook
```

- [ ] **Step 5: Commit**

```bash
git add src/ingest/hook.rs
git commit -m "feat(ingest): hook::store — raw + observed + self-heal graph rebuild"
```

---

## Task 4: Graph builder — external hook merge_keys

**Files:**
- Modify: `src/graph/build.rs`
- Test: `tests/graph_build.rs` (extend existing test file)

- [ ] **Step 1: Failing test**

Add to `tests/graph_build.rs`:

```rust
#[tokio::test]
async fn external_hook_nodes_use_hook_name_and_tool_use_id_merge_keys() {
    use witmcc::model::observed::*;
    use witmcc::graph::build::compute;
    use chrono::Utc;
    use serde_json::json;

    let session = "sess_HK";
    let ev = ObservedEvent {
        event_id: "ev1".into(),
        session_id: session.into(),
        observed_at: Utc::now(),
        actor: Actor::Hook,
        kind: EventKind::HookEvent,
        subkind: Some("pre_tool_use".into()),
        tool_use_id: Some("toolu_01".into()),
        payload: json!({"hook": {"hook_event_name": "PreToolUse"}}),
        parser_version: "hook@0.1.0".into(),
        ..Default::default()
    };
    let (nodes, _) = compute(session, &[ev]);
    assert_eq!(nodes.len(), 1);
    let n = &nodes[0];
    assert_eq!(n.node_kind, "hook_event");
    assert_eq!(n.merge_keys.get("hook_event_name").and_then(|v| v.as_str()), Some("pre_tool_use"));
    assert_eq!(n.merge_keys.get("tool_use_id").and_then(|v| v.as_str()), Some("toolu_01"));
    assert!(n.merge_keys.get("event_uuid").is_none(), "external hook must not key by event_uuid");
}

#[tokio::test]
async fn transcript_internal_hook_keeps_event_uuid_merge_keys() {
    use witmcc::model::observed::*;
    use witmcc::graph::build::compute;
    use chrono::Utc;
    use serde_json::json;

    let session = "sess_HK";
    let ev = ObservedEvent {
        event_id: "ev1".into(),
        session_id: session.into(),
        event_uuid: Some("uuid-abc".into()),
        observed_at: Utc::now(),
        actor: Actor::Hook,
        kind: EventKind::HookEvent,
        subkind: Some("hook_additional_context".into()),
        payload: json!({}),
        parser_version: "transcript@0.1.0".into(),
        ..Default::default()
    };
    let (nodes, _) = compute(session, &[ev]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].merge_keys.get("event_uuid").and_then(|v| v.as_str()), Some("uuid-abc"));
}
```

- [ ] **Step 2: Run, confirm fail**

```bash
cargo test --test graph_build external_hook_nodes_use_hook_name_and_tool_use_id_merge_keys
```
Expected: FAIL — current branch hard-codes `event_uuid` merge_keys for all `HookEvent` records.

- [ ] **Step 3: Branch on `parser_version`**

In `src/graph/build.rs::compute`, replace the existing `EventKind::HookEvent` arm (around line 52):

```rust
EventKind::HookEvent => {
    if e.parser_version.starts_with("hook@") {
        // External hook (slice-4): merge by (session, hook_event_name, tool_use_id).
        (
            "hook_event",
            json!({
                "session_id":      session_id,
                "hook_event_name": e.subkind,
                "tool_use_id":     e.tool_use_id,
            }),
        )
    } else {
        // Transcript-internal hook attachment (slice-1): keep event_uuid merge.
        (
            "hook_event",
            json!({"session_id": session_id, "event_uuid": e.event_uuid}),
        )
    }
}
```

- [ ] **Step 4: Run, confirm pass**

```bash
cargo test --test graph_build
```
Expected: both new tests pass; existing transcript regression tests pass unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/graph/build.rs tests/graph_build.rs
git commit -m "feat(graph): external hook nodes merge by (session, hook_event_name, tool_use_id)"
```

---

## Task 5: API DTO + handler + route

**Files:**
- Modify: `src/api/dto.rs`
- Create: `src/api/hook.rs`
- Modify: `src/api/mod.rs` (route registration + module)

- [ ] **Step 1: Failing handler test (via tests/api.rs)**

Add to `tests/api.rs` (use the existing axum-test harness pattern):

```rust
#[tokio::test]
async fn hook_post_accepts_single_pretooluse() {
    let server = make_server().await;
    let body = serde_json::json!({
        "session_id":      "sess_HK1",
        "hook_event_name": "PreToolUse",
        "tool_name":       "Bash",
        "tool_input":      {"command": "ls"},
        "tool_use_id":     "toolu_HK1"
    });
    let resp = server.post("/hooks/v1/events").json(&body).await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_events"], 1);
    assert_eq!(v["data"]["rejected_events"], 0);
    assert_eq!(v["data"]["duplicate_events"], 0);
    assert_eq!(v["data"]["sessions_touched"][0], "sess_HK1");

    let detail = server.get("/v1/sessions/sess_HK1").await;
    detail.assert_status_ok();
    let dv: serde_json::Value = detail.json();
    let has_hook = dv["data"]["events"].as_array().unwrap().iter().any(|e| {
        e["kind"] == "hook_event" && e["subkind"] == "pre_tool_use"
    });
    assert!(has_hook, "hook_event with subkind=pre_tool_use missing");
}

#[tokio::test]
async fn hook_post_rejects_missing_session_id() {
    let server = make_server().await;
    let body = serde_json::json!({
        "hook_event_name": "PreToolUse"
    });
    let resp = server.post("/hooks/v1/events").json(&body).await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["data"]["accepted_events"], 0);
    assert_eq!(v["data"]["rejected_events"], 1);
}
```

- [ ] **Step 2: Confirm fail**

```bash
cargo test --test api hook_post_accepts_single_pretooluse
```
Expected: 404 from server (route missing) or compile error.

- [ ] **Step 3: Implement**

`src/api/dto.rs` — append:

```rust
#[derive(Debug, Serialize)]
pub struct HookIngestResponse {
    pub accepted_events:  u64,
    pub rejected_events:  u64,
    pub duplicate_events: u64,
    pub sessions_touched: Vec<String>,
}
```

`src/api/hook.rs` — create:

```rust
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use sqlx::SqlitePool;

use crate::api::dto::HookIngestResponse;
use crate::ingest::hook;
use crate::model::meta::{Envelope, ResponseMeta};

const MAX_HOOK_BODY: usize = 1 * 1024 * 1024;

pub async fn ingest_events(
    State(pool): State<SqlitePool>,
    body: axum::body::Bytes,
) -> Result<Json<Envelope<HookIngestResponse>>, (StatusCode, Json<serde_json::Value>)> {
    if body.len() > MAX_HOOK_BODY {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "type": "about:blank",
                "title": "PAYLOAD_TOO_LARGE",
                "detail": format!("body exceeds {} bytes", MAX_HOOK_BODY),
            })),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "about:blank",
                "title": "BAD_REQUEST",
                "detail": format!("json parse error: {err}"),
            })),
        )
    })?;
    let parsed = hook::parse_body(&value);
    let result = hook::store(&pool, parsed, chrono::Utc::now()).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "type": "about:blank",
                "title": "DB_FAILURE",
                "detail": format!("{err}"),
            })),
        )
    })?;

    Ok(Json(Envelope {
        meta: ResponseMeta::now(),
        data: HookIngestResponse {
            accepted_events:  result.accepted_events,
            rejected_events:  result.rejected_events,
            duplicate_events: result.duplicate_events,
            sessions_touched: result.sessions_touched,
        },
    }))
}
```

`src/api/mod.rs` — add `pub mod hook;` and append the route:

```rust
.route("/hooks/v1/events", post(hook::ingest_events))
```

- [ ] **Step 4: Run, confirm pass**

```bash
cargo test --test api hook_post
```

- [ ] **Step 5: Commit**

```bash
git add src/api/hook.rs src/api/mod.rs src/api/dto.rs tests/api.rs
git commit -m "feat(api): POST /hooks/v1/events — Claude Code hook receiver (single + batch)"
```

---

## Task 6: Hook fixtures (9 known + batch + reject + unknown)

**Files:**
- Create: 11 files under `tests/fixtures/hook/`

- [ ] **Step 1: Add a parse-coverage test**

Append to `src/ingest/hook.rs` tests:

```rust
#[test]
fn fixtures_parse_with_expected_counts() {
    let cases = &[
        ("tests/fixtures/hook/pre_tool_use.json",       1usize, 0usize),
        ("tests/fixtures/hook/post_tool_use.json",      1, 0),
        ("tests/fixtures/hook/user_prompt_submit.json", 1, 0),
        ("tests/fixtures/hook/notification.json",       1, 0),
        ("tests/fixtures/hook/pre_compact.json",        1, 0),
        ("tests/fixtures/hook/session_start.json",      1, 0),
        ("tests/fixtures/hook/session_end.json",        1, 0),
        ("tests/fixtures/hook/stop.json",               1, 0),
        ("tests/fixtures/hook/subagent_stop.json",      1, 0),
        ("tests/fixtures/hook/batch_three.json",        3, 0),
        ("tests/fixtures/hook/missing_session_id.json", 0, 1),
        ("tests/fixtures/hook/unknown_event.json",      1, 0),
    ];
    for (path, ok, rej) in cases {
        let body: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let res = parse_body(&body);
        assert_eq!(res.events.len(), *ok, "{path} accepted");
        assert_eq!(res.rejected.len(), *rej, "{path} rejected");
    }
}
```

- [ ] **Step 2: Confirm fail**

```bash
cargo test --lib ingest::hook fixtures_parse_with_expected_counts
```
Expected: FAIL — files missing.

- [ ] **Step 3: Add fixtures**

`tests/fixtures/hook/pre_tool_use.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "PreToolUse",
  "tool_name": "Bash",
  "tool_input": {"command": "ls -la"},
  "tool_use_id": "toolu_pre_01"
}
```

`tests/fixtures/hook/post_tool_use.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "PostToolUse",
  "tool_name": "Bash",
  "tool_input": {"command": "ls -la"},
  "tool_response": {"stdout": "total 0\n", "stderr": "", "exit_code": 0},
  "tool_use_id": "toolu_pre_01"
}
```

`tests/fixtures/hook/user_prompt_submit.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "show me the current directory"
}
```

`tests/fixtures/hook/notification.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "Notification",
  "message": "Claude wants to run a Bash command — approve?"
}
```

`tests/fixtures/hook/pre_compact.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "PreCompact",
  "trigger": "auto"
}
```

`tests/fixtures/hook/session_start.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "SessionStart",
  "source": "startup"
}
```

`tests/fixtures/hook/session_end.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "SessionEnd"
}
```

`tests/fixtures/hook/stop.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "Stop"
}
```

`tests/fixtures/hook/subagent_stop.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "SubagentStop"
}
```

`tests/fixtures/hook/batch_three.json`:

```json
[
  {"session_id": "sess_fix_B", "hook_event_name": "PreToolUse",  "tool_name": "Bash", "tool_use_id": "toolu_b_01"},
  {"session_id": "sess_fix_B", "hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_use_id": "toolu_b_01", "tool_response": {"exit_code": 0}},
  {"session_id": "sess_fix_B", "hook_event_name": "Stop"}
]
```

`tests/fixtures/hook/missing_session_id.json`:

```json
{
  "session_id": "",
  "hook_event_name": "PreToolUse"
}
```

`tests/fixtures/hook/unknown_event.json`:

```json
{
  "session_id": "sess_fix_A",
  "hook_event_name": "FutureHook",
  "data": {"opaque": true}
}
```

- [ ] **Step 4: Run, confirm pass**

```bash
cargo test --lib ingest::hook fixtures_parse_with_expected_counts
```

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/hook
git commit -m "test(hook): JSON fixtures for 9 known events + batch + reject + unknown"
```

---

## Task 7: End-to-end ingest tests (`tests/hook_ingest.rs`)

**Files:**
- Create: `tests/hook_ingest.rs`

- [ ] **Step 1: Write the test file (initially failing because route not yet asserted via this file)**

```rust
// tests/hook_ingest.rs
mod common; // reuse make_server / make_pool helpers if collocated in tests/common.rs;
            // otherwise inline a local helper matching tests/api.rs.

use serde_json::Value;

fn load(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn batch_three_ingests_all_and_graph_has_three_hook_nodes() {
    let server = common::make_server().await;
    let body = load("tests/fixtures/hook/batch_three.json");
    let resp = server.post("/hooks/v1/events").json(&body).await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert_eq!(v["data"]["accepted_events"], 3);
    assert_eq!(v["data"]["rejected_events"], 0);

    let graph: Value = server.get("/v1/sessions/sess_fix_B/graph").await.json();
    let hook_count = graph["data"]["nodes"]
        .as_array().unwrap()
        .iter()
        .filter(|n| n["node_kind"] == "hook_event")
        .count();
    assert_eq!(hook_count, 3);
}

#[tokio::test]
async fn duplicate_post_increments_duplicate_events_and_keeps_one_row() {
    let server = common::make_server().await;
    let body = load("tests/fixtures/hook/pre_tool_use.json");

    let r1: Value = server.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r1["data"]["accepted_events"], 1);
    assert_eq!(r1["data"]["duplicate_events"], 0);

    let r2: Value = server.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r2["data"]["accepted_events"], 0);
    assert_eq!(r2["data"]["duplicate_events"], 1);
    // Self-heal still marks the session touched even on full dup.
    assert_eq!(r2["data"]["sessions_touched"][0], "sess_fix_A");

    let detail: Value = server.get("/v1/sessions/sess_fix_A").await.json();
    let cnt = detail["data"]["events"].as_array().unwrap().iter()
        .filter(|e| e["kind"] == "hook_event").count();
    assert_eq!(cnt, 1);
}

#[tokio::test]
async fn unknown_hook_event_name_accepts_with_unknown_subkind() {
    let server = common::make_server().await;
    let body = load("tests/fixtures/hook/unknown_event.json");
    let r: Value = server.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r["data"]["accepted_events"], 1);

    let detail: Value = server.get("/v1/sessions/sess_fix_A").await.json();
    let unknown = detail["data"]["events"].as_array().unwrap().iter()
        .any(|e| e["kind"] == "hook_event" && e["subkind"] == "unknown");
    assert!(unknown);
}

#[tokio::test]
async fn raw_endpoint_returns_original_hook_json() {
    let server = common::make_server().await;
    let body = load("tests/fixtures/hook/notification.json");
    server.post("/hooks/v1/events").json(&body).await;

    let detail: Value = server.get("/v1/sessions/sess_fix_A").await.json();
    let event_id = detail["data"]["events"].as_array().unwrap().iter()
        .find(|e| e["kind"] == "hook_event" && e["subkind"] == "notification")
        .unwrap()["event_id"].as_str().unwrap().to_string();

    let raw: Value = server.get(&format!("/v1/events/{event_id}/raw")).await.json();
    assert_eq!(raw["data"]["source"]["kind"], "hook");
    assert_eq!(raw["data"]["record"]["hook_event_name"], "Notification");
    assert_eq!(raw["data"]["record"]["message"], "Claude wants to run a Bash command — approve?");
}
```

If `tests/common.rs` doesn't yet exist with `make_server`, factor it out from `tests/api.rs` in this commit (small refactor): extract the helper functions there into `tests/common/mod.rs` and have both files use them. Verify `tests/api.rs` still compiles and passes.

- [ ] **Step 2: Confirm fail (or compile)**

```bash
cargo test --test hook_ingest
```

- [ ] **Step 3: Get all four tests passing**

The implementation from Tasks 2–5 should already pass these; if any fails, debug. Common pitfalls:
- `raw_endpoint_returns_original_hook_json`: `event_raw` uses `repo_raw::get_for_event_id` which joins observed → raw. Verify the raw row's `payload` field is byte-identical to the original input (canonical_json sorts keys, so reading it back yields a canonicalised but semantically-equal JSON — adjust the assertion to compare specific fields, not full equality).

- [ ] **Step 4: Confirm pass**

```bash
cargo test --test hook_ingest
cargo test  # full suite
```

- [ ] **Step 5: Commit**

```bash
git add tests/hook_ingest.rs tests/common* 2>/dev/null
git commit -m "test(hook): end-to-end ingest, dedup, unknown subkind, raw endpoint"
```

---

## Task 8: UI lane mapping — add 'Hook' lane

**Files:**
- Modify: `webui/src/api/laneMapping.ts`
- Create/Modify: `webui/src/api/__tests__/laneMapping.test.ts`

- [ ] **Step 1: Failing test**

Create `webui/src/api/__tests__/laneMapping.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { LANES, laneForNodeKind } from '../laneMapping';

describe('laneMapping', () => {
  it('exposes Hook as the 7th lane', () => {
    expect(LANES).toContain('Hook');
    expect(LANES.length).toBe(7);
  });
  it('maps hook_event to Hook', () => {
    expect(laneForNodeKind('hook_event')).toBe('Hook');
  });
  it('keeps existing mappings intact', () => {
    expect(laneForNodeKind('user_message')).toBe('Intent');
    expect(laneForNodeKind('otel_span')).toBe('OTel');
  });
});
```

- [ ] **Step 2: Confirm fail**

```bash
cd webui && npx vitest run src/api/__tests__/laneMapping.test.ts
```
Expected: FAIL — Hook lane missing.

- [ ] **Step 3: Update mapping**

`webui/src/api/laneMapping.ts`:

```ts
export const LANES = [
  'Intent',
  'Context',
  'Action',
  'State',
  'Hook',
  'OTel',
  'Quality',
] as const;
export type Lane = (typeof LANES)[number];

export function laneForNodeKind(kind: string): Lane | null {
  switch (kind) {
    case 'user_message':            return 'Intent';
    case 'assistant_message':       return 'Context';
    case 'tool_call':                return 'Action';
    case 'tool_result':              return 'Action';
    case 'hook_event':               return 'Hook';
    case 'file_history_snapshot':    return 'State';
    case 'otel_span':                return 'OTel';
    default:                         return null;
  }
}
```

- [ ] **Step 4: Confirm pass + regression**

```bash
cd webui && npx vitest run
```
All tests pass including Timeline regression (it iterates `LANES` so a 7th row appears naturally).

If `Timeline.test.tsx` hard-codes `lanes.length === 6`, update it to `7` in this commit; if it asserts the lane *names* explicitly, append `'Hook'` between `State` and `OTel` (matching the new constant order).

- [ ] **Step 5: Commit**

```bash
git add webui/src/api/laneMapping.ts webui/src/api/__tests__/laneMapping.test.ts webui/src/components/__tests__/Timeline.test.tsx 2>/dev/null
git commit -m "feat(webui): add Hook lane (7th); map hook_event -> Hook"
```

---

## Task 9: SourcePanel — hook record rendering

**Files:**
- Modify: `webui/src/components/SourcePanel.tsx`
- Modify: `webui/src/components/__tests__/SourcePanel.test.tsx`

- [ ] **Step 1: Failing tests**

Add to `webui/src/components/__tests__/SourcePanel.test.tsx`:

```tsx
it('renders pre_tool_use hook with tool_input section', async () => {
  // Mock raw fetch return:
  const rawResponse = {
    meta: { schema_version: '0.3.0', collection_profile: 'local_transcript_slice1', generated_at: '2026-05-19T00:00:00Z', next_cursor: null },
    data: {
      schema_version: '0.3.0',
      event_id: 'ev1',
      session_id: 'sess_X',
      source: { kind: 'hook', file_path: 'hook://sess_X/PreToolUse/toolu_01', line_no: 0, ingested_at: '2026-05-19T00:00:00Z' },
      record: {
        session_id: 'sess_X',
        hook_event_name: 'PreToolUse',
        tool_name: 'Bash',
        tool_input: { command: 'ls' },
        tool_use_id: 'toolu_01',
      },
      record_type: 'hook_event',
      redaction_state: 'none',
    },
  };
  // ... render SourcePanel with mocked fetch ...
  expect(screen.getByText('PreToolUse')).toBeInTheDocument();
  expect(screen.getByText(/Bash/)).toBeInTheDocument();
  expect(screen.getByText(/tool_input/i)).toBeInTheDocument();
});

it('renders notification hook with message text', async () => {
  // similar with record.hook_event_name='Notification', record.message='msg-text'
  expect(screen.getByText('Notification')).toBeInTheDocument();
  expect(screen.getByText('msg-text')).toBeInTheDocument();
});
```

(Match the existing OTel test's mocking pattern in this file — fetch stub + render flow.)

- [ ] **Step 2: Confirm fail**

```bash
cd webui && npx vitest run src/components/__tests__/SourcePanel.test.tsx
```

- [ ] **Step 3: Implement panel branch**

In `webui/src/components/SourcePanel.tsx`, add a branch when `record_type === 'hook_event'`. Pattern:

```tsx
function HookSection({ record }: { record: any }) {
  const name = record?.hook_event_name as string | undefined;
  if (!name) return null;
  return (
    <section className={styles.facetSection}>
      <h4>{name}</h4>
      <dl>
        {record.tool_name && <><dt>tool_name</dt><dd>{record.tool_name}</dd></>}
        {record.tool_use_id && <><dt>tool_use_id</dt><dd>{record.tool_use_id}</dd></>}
      </dl>
      {record.tool_input && (
        <details open><summary>tool_input</summary>
          <JsonView value={record.tool_input} />
        </details>
      )}
      {record.tool_response && (
        <details><summary>tool_response</summary>
          <JsonView value={record.tool_response} />
        </details>
      )}
      {record.prompt && <pre className={styles.prompt}>{record.prompt}</pre>}
      {record.message && <p className={styles.message}>{record.message}</p>}
      {record.trigger && <p><strong>trigger:</strong> {record.trigger}</p>}
      {record.source && <p><strong>source:</strong> {record.source}</p>}
    </section>
  );
}
```

Insert `{recordType === 'hook_event' && <HookSection record={raw.record} />}` adjacent to the existing OTel attributes section, before the catch-all `<JsonView>` for the full record.

- [ ] **Step 4: Confirm pass**

```bash
cd webui && npx vitest run
```

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/SourcePanel.tsx webui/src/components/__tests__/SourcePanel.test.tsx
git commit -m "feat(webui): SourcePanel renders hook record header + subkind-specific section"
```

---

## Task 10: Timeline regression — hook_event marker on Hook lane

**Files:**
- Modify: `webui/src/components/__tests__/Timeline.test.tsx`

- [ ] **Step 1: Failing assertion**

Append to the existing Timeline test file:

```tsx
it('places hook_event nodes on the Hook lane', () => {
  const nodes = [
    { node_id: 'n1', node_kind: 'hook_event', started_at: '2026-05-19T00:00:00Z' },
  ];
  const { container } = render(<Timeline nodes={nodes as any} edges={[]} onNodeClick={() => {}} />);
  // Use the same selector the OTel test uses — e.g. data-lane="Hook"
  const lane = container.querySelector('[data-lane="Hook"]');
  expect(lane).not.toBeNull();
  expect(lane?.querySelector('[data-node-id="n1"]')).not.toBeNull();
});
```

- [ ] **Step 2: Confirm fail**

```bash
cd webui && npx vitest run src/components/__tests__/Timeline.test.tsx
```

- [ ] **Step 3: Make it pass**

Timeline likely already routes via `laneForNodeKind`, so adding the Hook lane in Task 8 makes this pass without code change. If Timeline hard-codes lane positions or counts, update accordingly (e.g. add Hook between State and OTel rows).

- [ ] **Step 4: Run all webui tests**

```bash
cd webui && npx vitest run
```

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/__tests__/Timeline.test.tsx webui/src/components/Timeline.tsx webui/src/components/Timeline.module.css 2>/dev/null
git commit -m "test(webui): regression — hook_event marker renders on Hook lane"
```

---

## Task 11: README — hook collector forward-script docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: No test (docs-only). Skip Steps 1–2.**

- [ ] **Step 3: Add a "Hook collector" section after the OTel section**

Suggested content (adjust prose to match repo voice):

```md
### Hook collector (slice-4)

Capture live Claude Code hook lifecycle events (PreToolUse, PostToolUse,
UserPromptSubmit, Notification, PreCompact, SessionStart, SessionEnd, Stop,
SubagentStop) by registering a forward script in your `~/.claude/settings.json`:

```jsonc
{
  "hooks": {
    "PreToolUse":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "Notification": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "PreCompact":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SessionStart":[{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SessionEnd":  [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "Stop":        [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }],
    "SubagentStop":[{ "hooks": [{ "type": "command", "command": "/usr/local/bin/witmcc-forward.sh" }] }]
  }
}
```

`witmcc-forward.sh`:

```bash
#!/bin/bash
exec curl -sS -m 2 -X POST \
  -H 'content-type: application/json' \
  --data-binary @- \
  http://127.0.0.1:7878/hooks/v1/events > /dev/null 2>&1 || true
```

The 2-second timeout and `|| true` together implement **fail-soft degrade
semantics** (PRD OBS-3): if the witmcc receiver is down or slow, your Claude
Code session is never blocked.

> **Note:** witmcc does **not** install this script automatically. Hook event
> payloads can carry secrets (prompt text, command output); redaction is M7.
> Until then, only enable forwarding in trusted contexts.
```

- [ ] **Step 4: No automated check; manual review.**

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs(slice-4): README hook collector section + forward script"
```

---

## Task 12: Implementation notes — slice-4 section

**Files:**
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: No automated test. Skip Steps 1–2.**

- [ ] **Step 3: Append slice-4 sections**

Update the `localnav` to include:

```html
<a href="#slice4-overview">Overview (slice-4)</a>
<a href="#slice4-deviations">Intentional Deviations (slice-4)</a>
<a href="#slice4-commits">Commit Reference (slice-4)</a>
```

Append after the slice-3 section:

```html
<section id="slice4-overview">
  <h2><span class="num">10</span>Slice-4 Overview</h2>
  <p><code>slice4-hook-collector</code> branch — <code>POST /hooks/v1/events</code> receiver for live Claude Code hook lifecycle events. SCHEMA_VERSION 0.2.0 → 0.3.0.</p>
  <div class="callout good">
    <strong>전체 상태:</strong> slice-4 완료. 9개 hook event 모두 ingest, dedup self-heal 동작, Hook lane(7번째)에 timeline 마커 표시.
  </div>
</section>

<section id="slice4-deviations">
  <h2><span class="num">11</span>Slice-4 Intentional Deviations</h2>
  <!-- Add DEV-S4-XX items for each deviation encountered during implementation;
       prime candidates:
       - DEV-S4-01: External vs internal hook merge_keys split (parser_version branching in graph builder)
       - DEV-S4-02: Pass-through body shape (no wrapper envelope)
       - DEV-S4-03: 1 MB body limit (vs OTel's 4 MB)
       - DEV-S4-04: User-side forward script not installed automatically (CLAUDE.md non-goal)
       - DEV-S4-05: Documented dedup gap with transcript-internal hook attachments
  -->
</section>

<section id="slice4-commits">
  <h2><span class="num">12</span>Slice-4 Commit Reference</h2>
  <ul>
    <!-- Fill in commit SHA list at PR-prep time. Format mirrors slice-3 commits section. -->
  </ul>
</section>
```

(Adjust the `<span class="num">` numbering to continue from the existing slice-3 09 section.)

Fill in actual DEV-S4-XX items based on what surfaced during implementation; populate the commit list right before the PR.

- [ ] **Step 4: Manual review.**

- [ ] **Step 5: Commit**

```bash
git add docs/implementation-notes.html
git commit -m "docs(slice-4): implementation-notes section + deviations"
```

---

## Final Verification

```bash
# Backend
cargo build
cargo test                 # full suite — all green
cargo test --test '*'      # integration suite explicitly

# WebUI
cd webui && npx vitest run && cd ..

# Manual smoke
just webui-build && cargo build
./target/debug/witmcc serve --bind 127.0.0.1 --port 7878 &
sleep 1
curl -sS -X POST http://127.0.0.1:7878/hooks/v1/events \
  -H 'content-type: application/json' \
  --data-binary @tests/fixtures/hook/pre_tool_use.json | jq

curl -sS -X POST http://127.0.0.1:7878/hooks/v1/events \
  -H 'content-type: application/json' \
  --data-binary @tests/fixtures/hook/batch_three.json | jq

curl -sS http://127.0.0.1:7878/v1/sessions/sess_fix_A \
  | jq '.data.events[] | select(.kind=="hook_event") | {subkind, tool_use_id, tool_name}'

curl -sS http://127.0.0.1:7878/v1/sessions/sess_fix_B/graph \
  | jq '.data.nodes[] | select(.node_kind=="hook_event") | .merge_keys'

# UI: open http://127.0.0.1:7878/ → sess_fix_B → see hook markers on Hook lane → click → SourcePanel renders header + JSON.
kill %1
```

---

## Self-Review Checklist

Map each spec AC to the task that delivers it:

- AC §16.1 (POST single PreToolUse → 200, accepted=1) — **Task 5**
- AC §16.2 (POST batch of 3 → 200, accepted=3) — **Task 7**
- AC §16.3 (session shows up in `/v1/sessions`) — **Tasks 3, 5, 7** (via repo_observed + handler)
- AC §16.4 (events list includes hook records with correct subkind for all 9 names) — **Tasks 2, 6**
- AC §16.5 (graph contains hook_event nodes with merge_keys.hook_event_name) — **Tasks 4, 7**
- AC §16.6 (re-POST increments duplicate_events; sessions_touched still includes session) — **Tasks 3, 7**
- AC §16.7 (rejected events counted; valid batch items still ingest) — **Tasks 2, 6**
- AC §16.8 (SourcePanel + Timeline render hook records on Hook lane) — **Tasks 8, 9, 10**
- AC §16.9 (README forward-script with degrade semantics) — **Task 11**
- AC §16.10 (all prior tests pass; new tests ≥6 integration + ≥6 unit) — **Final Verification**

Confirm before opening PR:

- [ ] `cargo test` green; count of tests ≥ previous baseline + 12.
- [ ] `npx vitest run` green; lane count = 7.
- [ ] No `Co-Authored-By` lines anywhere in commits (`git log --grep='Co-Authored-By' slice4-hook-collector` returns nothing).
- [ ] `docs/implementation-notes.html` DEV-S4-XX items reflect any decisions that diverged from this plan; commit SHA list populated.
- [ ] README hook section reviewed by user before merge.
- [ ] PR title mirrors slice-3: `feat: slice-4 hook collector (POST /hooks/v1/events)`.
