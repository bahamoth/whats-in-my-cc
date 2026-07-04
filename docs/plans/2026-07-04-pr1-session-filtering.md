# PR-1 세션 메시지 카드 필터링 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `GET /v1/sessions/:id/events`에 4축 필터(kind·role/origin·실행 결과·내용 검색)를 서버측으로 추가하고, 리플레이 UI에 FilterBar(숨김 방식·flat 모드·URL 동기화·SSE 백필 결합)를 붙인다. 스펙: `docs/specs/2026-07-04-session-detail-improvements.md` §1.

**Architecture:** 백엔드는 `EventFilter`(파라미터 파싱·검증) + `origin_of`(TS `messageOrigin` 이식) + 커서 순서 청크 스캔(repo에 술어 클로저 전달)으로 구현한다. SQL로 싸게 거를 수 있는 축(kind·tool_name)은 WHERE로 내리고 나머지는 Rust 술어. 프론트는 `FilterState`(URL 왕복) → `useSessionWindow`(모든 fetch에 필터 파라미터) → flat 모드 스트림 렌더로 흐른다. 기존 `kind=` 경로는 새 필터 경로로 통합하고 기존 테스트(`tests/session_events_kind_filter.rs`)를 무수정 통과시켜 계약을 보존한다.

**Tech Stack:** Rust(axum, sqlx/SQLite, serde) · TypeScript(React 18, TanStack Query/Virtual, vitest, react-router `useSearchParams`).

## Global Constraints

- **TDD red 우선**: 모든 태스크는 실패 테스트 → 빨강 확인 → 구현 → 초록 순서. 테스트 후행 커밋 금지.
- **Real-data anchoring**: origin 분류는 `tests/fixtures/transcripts/real/message_origin_v01.jsonl`로 잠근다(TS 쪽과 동일 fixture — 드리프트 방지).
- **커밋**: conventional commit, **AI footer 금지**(프로젝트 훅이 차단), PR 병합은 rebase — 여기선 커밋만.
- **표기 원칙**(대시보드 스펙 §0 계승): 판정 문장 금지, 미측정 `—`. FilterBar 카피도 관측 사실만.
- **i18n**: 새 키는 `webui/src/i18n/catalog/en.ts`·`ko.ts` 양쪽 동시 추가(parity 테스트가 잠금). 이번 PR은 `.tip` 키를 만들지 않는다(툴팁 없음 — YAGNI).
- **WebUI 변경은 브라우저 smoke 후 커밋** (Task 12; 운영 serve :7878 재시작 절대 금지).
- **버전 수동 수정 금지**: `Cargo.toml`·`webui/package.json`·`package-lock.json`은 release-please가 bump.
- 테스트 결과는 exit code + 명시적 failure grep으로 검증("0 failed" 확인), 요약만 믿지 않는다.

## 파일 구조 (전체 조망)

| 파일 | 역할 |
|---|---|
| Create `src/insight/event_filter.rs` | `Origin`·`origin_of`·`EventFilter`(파싱/검증/`matches`)·`FilterCtx` — api·db 양쪽이 소비하는 순수 로직 |
| Modify `src/insight/mod.rs` | `pub mod event_filter;` 등록 |
| Modify `src/db/repo_observed.rs` | `list_session_window_scan`(청크 스캔+술어) · `count_session_scan` 추가, `list_session_window_kinds` 제거 |
| Modify `src/db/repo_signal.rs` | `evidence_event_ids(pool, session_id)` 추가 |
| Modify `src/db/repo_verification_run.rs` | `status_by_trigger(pool, session_id)` 추가 |
| Modify `src/api/routes.rs` | `EventsQuery` 8필드 추가, `session_events` 필터 경로 배선, around×필터 400, `matched_count` |
| Modify `src/api/dto.rs` | `SessionEventsResponse.matched_count: Option<i64>` |
| Create `tests/session_events_filter.rs` | 필터 통합 테스트(정확성·AND/OR·페이징·400·matched_count) |
| Create `webui/src/components/replay/stream/filterState.ts` | `FilterState`·URL/쿼리 파라미터 왕복·`isFilterActive` |
| Modify `webui/src/api/client.ts`, `webui/src/api/types.ts` | `EventFilterParams`, `matched_count` 타입 |
| Modify `webui/src/hooks/useSessionWindow.ts` | `filter` 옵션(모든 fetch 적용·변경 시 리셋), `matchedCount` 노출 |
| Modify `webui/src/components/replay/stream/streamModel.ts` | `buildStreamModel(..., { flat })` — 그룹핑 비활성 |
| Modify `webui/src/components/replay/stream/MessageCard.tsx`, `ActivityStack.tsx` | flat 모드 출처 배지(⑂) |
| Create `webui/src/components/replay/stream/FilterBar.tsx` | 축별 칩 드롭다운 + 텍스트 검색 + 매칭 수 + 해제 알림 |
| Modify `webui/src/components/replay/stream/AutoscrollToggle.tsx` | 필터 활성 시 "새 이벤트 ↓" 라벨 |
| Modify `webui/src/routes/SessionDetailPage.tsx` | URL↔FilterState, FilterBar 장착, 점프 규칙 |
| Modify `webui/src/i18n/catalog/en.ts`, `ko.ts` | `filter.*`·`stream.newEvents` 키 |

Interfaces(태스크 간 계약)는 각 태스크에 명시. 실행 전 준비:

```bash
cd /Users/bahamoth/projects/whats-in-my-cc
git checkout main && git pull && git checkout -b feat/session-filtering
```

---

### Task 1: `origin_of` — TS messageOrigin의 Rust 이식 (real fixture 앵커)

**Files:**
- Create: `src/insight/event_filter.rs`
- Modify: `src/insight/mod.rs`
- Fixture(기존): `tests/fixtures/transcripts/real/message_origin_v01.jsonl`

**Interfaces:**
- Produces: `pub enum Origin { Human, Command, CommandOutput, Skill, System, Notification, Teammate }` (+`as_str()`/`parse()` — snake이 아닌 TS 원문 표기 `human`,`command`,`command-output`,`skill`,`system`,`notification`,`teammate`), `pub fn origin_of(payload: &serde_json::Value, is_meta: bool) -> Origin`.
- 이식 원본(정규식·판정 순서의 SSOT): `webui/src/components/replay/stream/messageOrigin.ts` 35~104행. 마커 우선, isMeta 후행. 판정 순서 변경 금지.

- [ ] **Step 1: 실패 테스트 작성** — `src/insight/event_filter.rs`를 테스트만 담아 생성하고 `src/insight/mod.rs`에 `pub mod event_filter;` 추가. 테스트는 두 겹: (a) 합성 payload 단위 케이스, (b) real fixture 스캔 invariant.

```rust
//! §1.3 (docs/specs/2026-07-04-session-detail-improvements.md) — 이벤트 필터.
//! `origin_of`는 webui/src/components/replay/stream/messageOrigin.ts의 결정론
//! 이식이다(마커 우선 → isMeta 후행). 두 구현은 같은 real fixture
//! (message_origin_v01.jsonl)로 앵커되어 드리프트가 테스트로 잡힌다.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_payload(s: &str) -> serde_json::Value {
        json!({ "content": s })
    }

    #[test]
    fn origin_marker_cases_match_ts_classifier() {
        // messageOrigin.ts 87~103행과 1:1 — 순서·마커 정규식 계약.
        assert_eq!(
            origin_of(&text_payload("<command-name>/model</command-name>"), false),
            Origin::Command
        );
        assert_eq!(
            origin_of(&text_payload("<local-command-stdout>ok</local-command-stdout>"), true),
            Origin::CommandOutput
        );
        assert_eq!(
            origin_of(&text_payload("[Request interrupted by user]"), false),
            Origin::System
        );
        assert_eq!(
            origin_of(&text_payload("<task-notification>\n<task-id>x</task-id>"), false),
            Origin::Notification
        );
        assert_eq!(
            origin_of(
                &text_payload("Another Claude session sent a message: <teammate-message teammate_id=\"lead\">hi"),
                false
            ),
            Origin::Teammate
        );
        assert_eq!(
            origin_of(&text_payload("Base directory for this skill: /x"), false),
            Origin::Skill
        );
        // 마커 없음 + isMeta=true → skill(주입), isMeta=false → human.
        assert_eq!(origin_of(&text_payload("fix the bug"), true), Origin::Skill);
        assert_eq!(origin_of(&text_payload("fix the bug"), false), Origin::Human);
        // <system-reminder> 래핑은 human 유지 (messageOrigin.ts 99~103행 계약).
        assert_eq!(
            origin_of(&text_payload("<system-reminder>x</system-reminder>\nfix it"), false),
            Origin::Human
        );
        // {"text": ...} 형태 payload(user_message content 배열 분해분)도 동일 판정.
        assert_eq!(
            origin_of(&json!({"text": "<command-name>/foo</command-name>"}), false),
            Origin::Command
        );
    }

    #[test]
    fn origin_real_fixture_invariants() {
        // message_origin_v01.jsonl — TS messageOrigin.test.ts와 같은 동결 표본.
        // invariant: fixture의 user 레코드 중 markerless·isMeta=false는 human,
        // <command-name> 선행 레코드는 command로 분류된다.
        let raw = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/transcripts/real/message_origin_v01.jsonl"
        ))
        .expect("fixture");
        let mut saw_human = false;
        let mut saw_command = false;
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let rec: serde_json::Value = serde_json::from_str(line).expect("jsonl");
            if rec.get("type").and_then(|t| t.as_str()) != Some("user") {
                continue;
            }
            let Some(content) = rec.pointer("/message/content") else { continue };
            let Some(text) = content.as_str() else { continue };
            let is_meta = rec.get("isMeta").and_then(|m| m.as_bool()).unwrap_or(false);
            let o = origin_of(&serde_json::json!({ "content": text }), is_meta);
            if text.trim_start().starts_with("<command-name>") {
                assert_eq!(o, Origin::Command, "command marker must classify as command");
                saw_command = true;
            } else if !is_meta
                && !super::has_scaffold_marker(text)
            {
                assert_eq!(o, Origin::Human, "markerless non-meta must stay human: {text:.60}");
                saw_human = true;
            }
        }
        assert!(saw_human && saw_command, "fixture must exercise both branches");
    }
}
```

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --lib insight::event_filter 2>&1 | tail -20`
Expected: 컴파일 실패 — `cannot find type Origin` / `cannot find function origin_of`.

- [ ] **Step 3: 최소 구현** — 같은 파일 상단에 추가. 정규식은 `regex` crate 대신 결정론 `str` 술어로 이식한다(모든 마커가 "선행 공백 후 고정 접두"라 substring 검사로 충분 — TS의 `^\s*` 앵커와 동치).

```rust
use serde_json::Value;

/// messageOrigin.ts `MessageOrigin`의 Rust 대응. 표기 문자열은 TS와 동일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Human,
    Command,
    CommandOutput,
    Skill,
    System,
    Notification,
    Teammate,
}

impl Origin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Origin::Human => "human",
            Origin::Command => "command",
            Origin::CommandOutput => "command-output",
            Origin::Skill => "skill",
            Origin::System => "system",
            Origin::Notification => "notification",
            Origin::Teammate => "teammate",
        }
    }
    pub fn parse(s: &str) -> Option<Origin> {
        Some(match s {
            "human" => Origin::Human,
            "command" => Origin::Command,
            "command-output" => Origin::CommandOutput,
            "skill" => Origin::Skill,
            "system" => Origin::System,
            "notification" => Origin::Notification,
            "teammate" => Origin::Teammate,
            _ => return None,
        })
    }
}

/// user_message payload의 원문 텍스트 — messageOrigin.ts `userMessageText`와
/// 동일 우선순위 (content(string) → text).
fn user_message_text(payload: &Value) -> &str {
    payload
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("text").and_then(|v| v.as_str()))
        .unwrap_or("")
}

fn starts_with_any(text: &str, prefixes: &[&str]) -> bool {
    let t = text.trim_start();
    prefixes.iter().any(|p| t.starts_with(p))
}

/// messageOrigin.ts `hasScaffoldMarker`의 이식 — 전 마커 합집합.
pub fn has_scaffold_marker(text: &str) -> bool {
    starts_with_any(
        text,
        &[
            "<command-name>",
            "<command-message>",
            "<command-args>",
            "<local-command-stdout>",
            "<local-command-caveat>",
            "[Request interrupted",
            "Base directory for this skill:",
            "<task-notification>",
            "<teammate-message",
            "Another Claude session sent a message:",
        ],
    )
}

/// messageOrigin.ts `messageOrigin`의 이식. 마커 판정 순서는 TS와 동일하게
/// 유지한다(순서가 계약) — command → command-output → system → notification
/// → teammate → skill-scaffold → isMeta → human.
pub fn origin_of(payload: &Value, is_meta: bool) -> Origin {
    let text = user_message_text(payload);
    if starts_with_any(text, &["<command-name>", "<command-message>", "<command-args>"]) {
        return Origin::Command;
    }
    if starts_with_any(text, &["<local-command-stdout>", "<local-command-caveat>"]) {
        return Origin::CommandOutput;
    }
    if starts_with_any(text, &["[Request interrupted"]) {
        return Origin::System;
    }
    if starts_with_any(text, &["<task-notification>"]) {
        return Origin::Notification;
    }
    // TS: /^\s*(?:Another Claude session sent a message:\s*)?<teammate-message[\s>]/
    {
        let t = text.trim_start();
        let t = t
            .strip_prefix("Another Claude session sent a message:")
            .map(str::trim_start)
            .unwrap_or(t);
        if let Some(rest) = t.strip_prefix("<teammate-message") {
            if rest.starts_with(char::is_whitespace) || rest.starts_with('>') {
                return Origin::Teammate;
            }
        }
    }
    if starts_with_any(text, &["Base directory for this skill:"]) {
        return Origin::Skill;
    }
    if is_meta {
        return Origin::Skill;
    }
    Origin::Human
}
```

- [ ] **Step 4: 초록 확인**

Run: `cargo test --lib insight::event_filter 2>&1 | tail -5`
Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 5: 커밋**

```bash
git add src/insight/event_filter.rs src/insight/mod.rs
git commit -m "feat(insight): origin_of — messageOrigin.ts의 Rust 이식 (real fixture 앵커)"
```

---

### Task 2: `EventFilter` — 파싱·검증·이벤트 술어

**Files:**
- Modify: `src/insight/event_filter.rs`

**Interfaces:**
- Consumes: Task 1의 `Origin`, `origin_of`.
- Produces:

```rust
pub struct RawFilterParams<'a> {          // routes.rs의 EventsQuery 필드를 그대로 받는 입력
    pub kind: Option<&'a str>, pub role: Option<&'a str>, pub origin: Option<&'a str>,
    pub error: Option<&'a str>, pub signal: Option<&'a str>, pub verification: Option<&'a str>,
    pub tool: Option<&'a str>, pub model: Option<&'a str>, pub q: Option<&'a str>,
}
pub struct EventFilter {
    pub kinds: Option<Vec<String>>,        // 검증된 EventKind snake_case
    pub roles: Option<Vec<Role>>,          // enum Role { User, Assistant, System }
    pub origins: Option<Vec<Origin>>,
    pub error: bool,
    pub signal: bool,
    pub verifications: Option<Vec<String>>, // "passed"|"failed"|"unknown"
    pub tools: Option<Vec<String>>,
    pub models: Option<Vec<String>>,
    pub q: Option<String>,                  // 소문자화 저장
}
impl EventFilter {
    pub fn from_params(p: &RawFilterParams) -> Result<Option<EventFilter>, String>; // Err = 400 detail
    pub fn needs_ctx(&self) -> bool;        // signal || verifications
    pub fn matches(&self, ev: &ObservedEvent, ctx: &FilterCtx) -> bool;
}
#[derive(Default)]
pub struct FilterCtx {
    pub signal_evidence: std::collections::HashSet<String>,             // event_id들
    pub verification_by_trigger: std::collections::HashMap<String, String>, // trigger_event_id → status
}
```

- 축 의미(스펙 §1.2 표): 축끼리 AND, CSV는 OR. `role`은 kind 매핑(user→`user_message`, assistant→`assistant_message`, system→`system_summary`). `error`는 `payload.tool_result.is_error == true`. `tool`은 `ObservedEvent.tool_name` 정확 일치. `model`은 `payload.model` 정확 일치. `q`는 다음 문자열 필드의 대소문자 무시 부분 일치: user_message `payload.content|text`, assistant_message `payload.text`, thinking `payload.thinking`, tool_call `payload.input` 직렬화 문자열 + `tool_name`, tool_result `payload.tool_result.content`(문자열 또는 `[{type:"text",text}]` 배열).

- [ ] **Step 1: 실패 테스트 작성** — `#[cfg(test)] mod tests`에 추가:

```rust
    use crate::model::observed::{EventKind, ObservedEvent};

    fn ev(kind: EventKind, payload: serde_json::Value) -> ObservedEvent {
        ObservedEvent { kind, payload, ..Default::default() }
    }

    #[test]
    fn from_params_rejects_unknown_values() {
        let bad_origin = RawFilterParams { origin: Some("alien"), ..Default::default() };
        assert!(EventFilter::from_params(&bad_origin).is_err());
        let bad_role = RawFilterParams { role: Some("bot"), ..Default::default() };
        assert!(EventFilter::from_params(&bad_role).is_err());
        let bad_ver = RawFilterParams { verification: Some("flaky"), ..Default::default() };
        assert!(EventFilter::from_params(&bad_ver).is_err());
        let bad_kind = RawFilterParams { kind: Some("nope"), ..Default::default() };
        assert!(EventFilter::from_params(&bad_kind).is_err());
        let bad_error = RawFilterParams { error: Some("yes"), ..Default::default() };
        assert!(EventFilter::from_params(&bad_error).is_err()); // "true"만 허용
        // 파라미터 전무 → None (필터 비활성)
        assert!(EventFilter::from_params(&RawFilterParams::default()).unwrap().is_none());
    }

    #[test]
    fn matches_axes_and_combination() {
        let ctx = FilterCtx::default();
        // role=user → user_message kind 매핑
        let f = EventFilter::from_params(&RawFilterParams { role: Some("user"), ..Default::default() })
            .unwrap().unwrap();
        assert!(f.matches(&ev(EventKind::UserMessage, serde_json::json!({"content":"hi"})), &ctx));
        assert!(!f.matches(&ev(EventKind::AssistantMessage, serde_json::json!({"text":"hi"})), &ctx));

        // error=true → tool_result.is_error
        let f = EventFilter::from_params(&RawFilterParams { error: Some("true"), ..Default::default() })
            .unwrap().unwrap();
        assert!(f.matches(
            &ev(EventKind::ToolResult, serde_json::json!({"tool_result":{"is_error":true,"content":"boom"}})),
            &ctx
        ));
        assert!(!f.matches(
            &ev(EventKind::ToolResult, serde_json::json!({"tool_result":{"is_error":false,"content":"ok"}})),
            &ctx
        ));

        // tool CSV OR (tool_name 컬럼)
        let f = EventFilter::from_params(&RawFilterParams { tool: Some("Bash,Edit"), ..Default::default() })
            .unwrap().unwrap();
        let mut e = ev(EventKind::ToolCall, serde_json::json!({"tool_name":"Bash","input":{}}));
        e.tool_name = Some("Bash".into());
        assert!(f.matches(&e, &ctx));
        e.tool_name = Some("Read".into());
        assert!(!f.matches(&e, &ctx));

        // model 정확 일치
        let f = EventFilter::from_params(&RawFilterParams { model: Some("claude-fable-5"), ..Default::default() })
            .unwrap().unwrap();
        assert!(f.matches(
            &ev(EventKind::AssistantMessage, serde_json::json!({"text":"x","model":"claude-fable-5"})),
            &ctx
        ));
        assert!(!f.matches(
            &ev(EventKind::AssistantMessage, serde_json::json!({"text":"x","model":"claude-haiku-4-5-20251001"})),
            &ctx
        ));

        // q — 대소문자 무시, tool_result content 배열 텍스트 포함
        let f = EventFilter::from_params(&RawFilterParams { q: Some("PaNiC"), ..Default::default() })
            .unwrap().unwrap();
        assert!(f.matches(
            &ev(EventKind::ToolResult,
                serde_json::json!({"tool_result":{"content":[{"type":"text","text":"thread panicked"}]}})),
            &ctx
        ));
        assert!(!f.matches(&ev(EventKind::UserMessage, serde_json::json!({"content":"calm"})), &ctx));

        // signal=true → ctx의 evidence set
        let f = EventFilter::from_params(&RawFilterParams { signal: Some("true"), ..Default::default() })
            .unwrap().unwrap();
        assert!(f.needs_ctx());
        let mut ctx2 = FilterCtx::default();
        ctx2.signal_evidence.insert("EV1".into());
        let mut e = ev(EventKind::ToolCall, serde_json::json!({}));
        e.event_id = "EV1".into();
        assert!(f.matches(&e, &ctx2));
        e.event_id = "EV2".into();
        assert!(!f.matches(&e, &ctx2));

        // verification=failed → ctx의 trigger 맵
        let f = EventFilter::from_params(&RawFilterParams { verification: Some("failed"), ..Default::default() })
            .unwrap().unwrap();
        let mut ctx3 = FilterCtx::default();
        ctx3.verification_by_trigger.insert("EV9".into(), "failed".into());
        let mut e = ev(EventKind::ToolCall, serde_json::json!({}));
        e.event_id = "EV9".into();
        assert!(f.matches(&e, &ctx3));

        // AND 조합: origin=human && q=deploy — 둘 다 만족해야 매칭
        let f = EventFilter::from_params(&RawFilterParams {
            origin: Some("human"), q: Some("deploy"), ..Default::default()
        }).unwrap().unwrap();
        assert!(f.matches(&ev(EventKind::UserMessage, serde_json::json!({"content":"deploy it"})), &ctx));
        assert!(!f.matches(&ev(EventKind::UserMessage, serde_json::json!({"content":"ship it"})), &ctx));
        assert!(!f.matches(
            &ev(EventKind::UserMessage, serde_json::json!({"content":"<task-notification>deploy</task-notification>"})),
            &ctx
        ));
    }
```

`RawFilterParams`에 `#[derive(Default)]`가 필요하다(테스트가 `..Default::default()` 사용).

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --lib insight::event_filter 2>&1 | tail -5`
Expected: 컴파일 실패 — `cannot find struct RawFilterParams` / `EventFilter`.

- [ ] **Step 3: 구현** — Interfaces 블록의 시그니처대로. 핵심 로직:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role { User, Assistant, System }

impl Role {
    fn parse(s: &str) -> Option<Role> {
        Some(match s { "user" => Role::User, "assistant" => Role::Assistant, "system" => Role::System, _ => return None })
    }
    /// role 축은 kind 매핑으로 판정한다(§1.2 — payload role 재파싱 불필요:
    /// ingest가 kind로 이미 분해). user→user_message, assistant→assistant_message,
    /// system→system_summary.
    fn kind_str(&self) -> &'static str {
        match self { Role::User => "user_message", Role::Assistant => "assistant_message", Role::System => "system_summary" }
    }
}

#[derive(Default)]
pub struct RawFilterParams<'a> { /* Interfaces 블록 그대로 */ }

fn csv(s: &str) -> Vec<&str> {
    s.split(',').map(str::trim).filter(|x| !x.is_empty()).collect()
}

impl EventFilter {
    pub fn from_params(p: &RawFilterParams) -> Result<Option<EventFilter>, String> {
        // kind: EventKind taxonomy 검증 (routes.rs의 기존 인라인 검증을 이관)
        let kinds = match p.kind {
            None => None,
            Some(s) => {
                let mut out = Vec::new();
                for k in csv(s) {
                    let ok = serde_json::from_value::<crate::model::observed::EventKind>(
                        serde_json::Value::String(k.to_string())).is_ok();
                    if !ok { return Err(format!("unknown event kind: {k}")); }
                    out.push(k.to_string());
                }
                if out.is_empty() { None } else { Some(out) }
            }
        };
        let roles = parse_csv_enum(p.role, Role::parse, "role")?;
        let origins = parse_csv_enum(p.origin, Origin::parse, "origin")?;
        let verifications = match p.verification {
            None => None,
            Some(s) => {
                let vals: Vec<String> = csv(s).iter().map(|v| v.to_string()).collect();
                for v in &vals {
                    if !matches!(v.as_str(), "passed" | "failed" | "unknown") {
                        return Err(format!("unknown verification outcome: {v}"));
                    }
                }
                if vals.is_empty() { None } else { Some(vals) }
            }
        };
        let error = parse_bool_flag(p.error, "error")?;
        let signal = parse_bool_flag(p.signal, "signal")?;
        let tools = p.tool.map(|s| csv(s).iter().map(|x| x.to_string()).collect::<Vec<_>>()).filter(|v: &Vec<String>| !v.is_empty());
        let models = p.model.map(|s| csv(s).iter().map(|x| x.to_string()).collect::<Vec<_>>()).filter(|v: &Vec<String>| !v.is_empty());
        let q = p.q.map(str::trim).filter(|s| !s.is_empty()).map(str::to_lowercase);
        let f = EventFilter { kinds, roles, origins, error, signal, verifications, tools, models, q };
        if f.kinds.is_none() && f.roles.is_none() && f.origins.is_none() && !f.error && !f.signal
            && f.verifications.is_none() && f.tools.is_none() && f.models.is_none() && f.q.is_none() {
            return Ok(None);
        }
        Ok(Some(f))
    }

    pub fn needs_ctx(&self) -> bool { self.signal || self.verifications.is_some() }

    pub fn matches(&self, ev: &ObservedEvent, ctx: &FilterCtx) -> bool {
        if let Some(kinds) = &self.kinds {
            if !kinds.iter().any(|k| k == ev.kind.as_str()) { return false; }
        }
        if let Some(roles) = &self.roles {
            if !roles.iter().any(|r| r.kind_str() == ev.kind.as_str()) { return false; }
        }
        if let Some(origins) = &self.origins {
            // origin은 user_message에만 정의 — 다른 kind는 origin 축에서 탈락.
            if ev.kind != crate::model::observed::EventKind::UserMessage { return false; }
            let o = origin_of(&ev.payload, ev.is_meta);
            if !origins.contains(&o) { return false; }
        }
        if self.error {
            let is_err = ev.payload.pointer("/tool_result/is_error").and_then(|v| v.as_bool()) == Some(true);
            if !is_err { return false; }
        }
        if self.signal && !ctx.signal_evidence.contains(&ev.event_id) { return false; }
        if let Some(outcomes) = &self.verifications {
            match ctx.verification_by_trigger.get(&ev.event_id) {
                Some(st) if outcomes.contains(st) => {}
                _ => return false,
            }
        }
        if let Some(tools) = &self.tools {
            match &ev.tool_name {
                Some(t) if tools.contains(t) => {}
                _ => return false,
            }
        }
        if let Some(models) = &self.models {
            match ev.payload.get("model").and_then(|v| v.as_str()) {
                Some(m) if models.iter().any(|x| x == m) => {}
                _ => return false,
            }
        }
        if let Some(q) = &self.q {
            if !searchable_text(ev).to_lowercase().contains(q.as_str()) { return false; }
        }
        true
    }
}

/// q 축의 검색 대상 문자열(§1.2): 메시지 본문 + 도구 입력/결과 문자열 필드.
fn searchable_text(ev: &ObservedEvent) -> String {
    let p = &ev.payload;
    let mut parts: Vec<String> = Vec::new();
    for key in ["content", "text", "thinking"] {
        if let Some(s) = p.get(key).and_then(|v| v.as_str()) { parts.push(s.to_string()); }
    }
    if let Some(t) = &ev.tool_name { parts.push(t.clone()); }
    if let Some(input) = p.get("input") {
        if !input.is_null() { parts.push(input.to_string()); }
    }
    if let Some(tr) = p.get("tool_result") {
        match tr.get("content") {
            Some(serde_json::Value::String(s)) => parts.push(s.clone()),
            Some(serde_json::Value::Array(items)) => {
                for it in items {
                    if let Some(s) = it.get("text").and_then(|v| v.as_str()) { parts.push(s.to_string()); }
                }
            }
            _ => {}
        }
    }
    parts.join("\n")
}

fn parse_csv_enum<T>(s: Option<&str>, parse: fn(&str) -> Option<T>, axis: &str) -> Result<Option<Vec<T>>, String> {
    match s {
        None => Ok(None),
        Some(raw) => {
            let mut out = Vec::new();
            for v in csv(raw) {
                out.push(parse(v).ok_or_else(|| format!("unknown {axis} value: {v}"))?);
            }
            Ok(if out.is_empty() { None } else { Some(out) })
        }
    }
}

fn parse_bool_flag(s: Option<&str>, axis: &str) -> Result<bool, String> {
    match s {
        None => Ok(false),
        Some("true") => Ok(true),
        Some(v) => Err(format!("{axis} accepts only 'true', got: {v}")),
    }
}
```

- [ ] **Step 4: 초록 확인**

Run: `cargo test --lib insight::event_filter 2>&1 | tail -5`
Expected: `test result: ok. 4 passed; 0 failed`.

- [ ] **Step 5: 커밋**

```bash
git add src/insight/event_filter.rs
git commit -m "feat(insight): EventFilter — 4축 파싱·검증·AND/OR 술어 (§1.2)"
```

---

### Task 3: FilterCtx 조회 repo 함수 2종

**Files:**
- Modify: `src/db/repo_signal.rs`
- Modify: `src/db/repo_verification_run.rs`

**Interfaces:**
- Produces: `repo_signal::evidence_event_ids(pool, session_id) -> Result<std::collections::HashSet<String>>` (signal.evidence_refs JSON 배열 — bare string 또는 `{event_id}` 객체 둘 다 수용: SessionDetailPage.tsx 218~227행과 동일 계약), `repo_verification_run::status_by_trigger(pool, session_id) -> Result<std::collections::HashMap<String, String>>` (trigger_event_id → status; 같은 trigger에 여러 run이면 **최신 started_at 우선**).

- [ ] **Step 1: 실패 테스트 작성** — 각 repo 파일의 기존 `#[cfg(test)]` 모듈에 추가(기존 테스트의 in-memory pool 셋업 헬퍼 재사용 — 파일 내 기존 테스트에서 migrate 패턴을 복사):

```rust
// 두 테스트 공용 pool 셋업(각 파일 tests 모듈에 이미 동형 헬퍼가 있으면 재사용):
async fn mem_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::db::migrate(&pool).await.unwrap();
    pool
}
```

```rust
// repo_signal.rs tests 모듈
async fn insert_signal_row(pool: &SqlitePool, session_id: &str, evidence_refs: &str) {
    // 스키마: migrations/20260607140000_0021_signal.sql — NOT NULL 컬럼만 채움.
    sqlx::query(
        "INSERT INTO signal (signal_id, schema_version, session_id, category, \
         evidence_refs, facts, provenance, created_at) \
         VALUES (?, 'signal.v1', ?, 'tool_failure', ?, '{}', 'l1_detector', datetime('now'))",
    )
    .bind(format!("sig_{session_id}_{}", evidence_refs.len()))
    .bind(session_id)
    .bind(evidence_refs)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn evidence_event_ids_parses_both_ref_shapes() {
    let pool = mem_pool().await;
    // evidence_refs: bare string + {event_id} 혼합 (SessionDetailPage와 동일 계약)
    insert_signal_row(&pool, "sess-f", r#"["EVA",{"event_id":"EVB"}]"#).await;
    insert_signal_row(&pool, "sess-other", r#"["EVX"]"#).await;
    let ids = evidence_event_ids(&pool, "sess-f").await.unwrap();
    assert_eq!(ids, ["EVA", "EVB"].into_iter().map(String::from).collect());
}
```

INSERT 컬럼 목록은 구현 시 `repo_signal.rs`의 기존 insert 함수(39행 부근)와 대조해 실제 스키마에 맞춘다(컬럼이 추가돼 있으면 그 insert 헬퍼를 그대로 호출하는 편이 낫다).

```rust
// repo_verification_run.rs tests 모듈
async fn insert_run(pool: &SqlitePool, session_id: &str, trigger: &str, status: &str, started_at: &str) {
    // 스키마: migrations/20260527120000_0005_verification_run.sql
    sqlx::query(
        "INSERT INTO verification_run (verification_run_id, session_id, source, command, \
         command_kind, trigger_event_id, status, started_at, raw_event_id, parser_version) \
         VALUES (?, ?, 'bash', 'cargo test', 'test_suite_rust', ?, ?, ?, 'raw_x', 'verification_run@v1')",
    )
    .bind(format!("vr_{trigger}_{started_at}"))
    .bind(session_id)
    .bind(trigger)
    .bind(status)
    .bind(started_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn status_by_trigger_latest_run_wins() {
    let pool = mem_pool().await;
    insert_run(&pool, "sess-f", "EV1", "failed", "2026-07-04T00:00:00Z").await;
    insert_run(&pool, "sess-f", "EV1", "passed", "2026-07-04T00:05:00Z").await; // 재실행 성공
    insert_run(&pool, "sess-f", "EV2", "unknown", "2026-07-04T00:01:00Z").await;
    insert_run(&pool, "sess-other", "EV1", "failed", "2026-07-04T00:00:00Z").await;
    let m = status_by_trigger(&pool, "sess-f").await.unwrap();
    assert_eq!(m.get("EV1").map(String::as_str), Some("passed"));
    assert_eq!(m.get("EV2").map(String::as_str), Some("unknown"));
    assert_eq!(m.len(), 2, "다른 세션 run은 제외");
}
```

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --lib db:: 2>&1 | tail -5`
Expected: 컴파일 실패 — `cannot find function evidence_event_ids` / `status_by_trigger`.

- [ ] **Step 3: 구현**

```rust
// repo_signal.rs
/// §1.2 signal=true 축 — 이 세션 시그널들의 evidence event_id 전체 집합.
/// evidence_refs는 JSON 배열(bare event_id string 또는 {event_id} 객체 혼재).
pub async fn evidence_event_ids(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<std::collections::HashSet<String>> {
    let rows = sqlx::query("SELECT evidence_refs FROM signal WHERE session_id = ?")
        .bind(session_id)
        .fetch_all(pool)
        .await?;
    let mut out = std::collections::HashSet::new();
    for r in rows {
        let raw: String = r.get("evidence_refs");
        let Ok(refs) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) else { continue };
        for v in refs {
            match v {
                serde_json::Value::String(s) => { out.insert(s); }
                serde_json::Value::Object(o) => {
                    if let Some(id) = o.get("event_id").and_then(|x| x.as_str()) {
                        out.insert(id.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    Ok(out)
}
```

```rust
// repo_verification_run.rs
/// §1.2 verification 축 — trigger_event_id → 최신 run status.
/// 같은 trigger의 재실행은 started_at 최신이 이긴다(마지막 판정이 현재 상태).
pub async fn status_by_trigger(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<std::collections::HashMap<String, String>> {
    let rows = sqlx::query(
        "SELECT trigger_event_id, status FROM verification_run \
         WHERE session_id = ? ORDER BY started_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        // ASC 순회 + insert 덮어쓰기 = 최신 started_at 승리.
        out.insert(r.get("trigger_event_id"), r.get("status"));
    }
    Ok(out)
}
```

- [ ] **Step 4: 초록 확인**

Run: `cargo test --lib db:: 2>&1 | tail -5`
Expected: `0 failed`, 새 테스트 2개 포함 전부 통과.

- [ ] **Step 5: 커밋**

```bash
git add src/db/repo_signal.rs src/db/repo_verification_run.rs
git commit -m "feat(db): FilterCtx 조회 — signal evidence 집합·verification trigger 상태 맵"
```

---

### Task 4: 커서 청크 스캔 repo — `list_session_window_scan` · `count_session_scan`

**Files:**
- Modify: `src/db/repo_observed.rs`

**Interfaces:**
- Consumes: `Cursor`(기존 `crate::model::cursor`), `ObservedEvent`.
- Produces:

```rust
/// 필터 창(§1.2 실행 전략): kind/tool은 SQL WHERE 푸시다운, 나머지는 호출자
/// 술어. 커서 순서로 CHUNK(1000)행씩 스캔하며 술어 통과분을 모으고 limit
/// 충족 시 중단. 반환 순서는 다른 창과 동일하게 ASC.
pub async fn list_session_window_scan(
    pool: &SqlitePool,
    session_id: &str,
    sql_kinds: Option<&[String]>,
    sql_tools: Option<&[String]>,
    pred: &dyn Fn(&ObservedEvent) -> bool,
    before: Option<&Cursor>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<ObservedEvent>>;

/// matched_count(§1.2): 같은 푸시다운+술어로 세션 전체 매칭 수를 센다.
pub async fn count_session_scan(
    pool: &SqlitePool,
    session_id: &str,
    sql_kinds: Option<&[String]>,
    sql_tools: Option<&[String]>,
    pred: &dyn Fn(&ObservedEvent) -> bool,
) -> Result<i64>;
```

- 스캔 방향: `before` 또는 무커서 → 최신부터 DESC 스캔 후 결과 reverse(ASC 와이어). `after`만 → ASC 스캔. `(before,after)` 동시 → ASC 스캔에 상한 경계 추가. 각 청크의 마지막 행 커서에서 이어서 다음 청크(매칭 여부와 무관하게 전진). 청크가 CHUNK행 미만이면 세션 소진.
- 필터 창은 unfiltered 창의 rendered-anchor 특례를 적용하지 않는다(호출자가 원하는 축을 이미 지정 — `list_session_window_kinds`와 같은 근거).

- [ ] **Step 1: 실패 테스트 작성** — `repo_observed.rs` 기존 `#[cfg(test)]` 모듈에(기존 창 테스트의 seed 헬퍼 패턴 재사용):

```rust
#[tokio::test]
async fn window_scan_pushdown_predicate_and_pagination() {
    let pool = test_pool().await; // 파일 내 기존 헬퍼. 없으면 migrate 직접.
    // 60행: user_message 20(그중 절반 payload에 "deploy") + tool_call 20(Bash/Edit 교차) + metric_sample 20
    seed_scan_session(&pool, "sess-scan").await;
    let deploy = |e: &ObservedEvent| {
        e.payload.get("content").and_then(|v| v.as_str()).is_some_and(|s| s.contains("deploy"))
    };
    // (1) 술어만: user_message 중 deploy 10건, limit 4 → 최신 4건 ASC
    let evs = list_session_window_scan(
        &pool, "sess-scan", Some(&["user_message".into()]), None, &deploy, None, None, 4,
    ).await.unwrap();
    assert_eq!(evs.len(), 4);
    assert!(evs.windows(2).all(|w| w[0].observed_at <= w[1].observed_at));
    assert!(evs.iter().all(|e| deploy(e)));
    // (2) before 커서로 과거 페이지: 누락·중복 없이 나머지 6건
    let c = Cursor { observed_at: evs[0].observed_at, event_id: evs[0].event_id.clone() };
    let older = list_session_window_scan(
        &pool, "sess-scan", Some(&["user_message".into()]), None, &deploy, Some(&c), None, 100,
    ).await.unwrap();
    assert_eq!(older.len(), 6);
    let mut all: Vec<&str> = older.iter().chain(evs.iter()).map(|e| e.event_id.as_str()).collect();
    let n = all.len();
    all.dedup();
    assert_eq!(n, all.len(), "no dup across page boundary");
    // (3) after 커서 전진: 소진 시 요청 미만 반환
    let last = evs.last().unwrap();
    let c2 = Cursor { observed_at: last.observed_at, event_id: last.event_id.clone() };
    let newer = list_session_window_scan(
        &pool, "sess-scan", Some(&["user_message".into()]), None, &deploy, None, Some(&c2), 100,
    ).await.unwrap();
    assert!(newer.is_empty());
    // (4) tool 푸시다운
    let any = |_: &ObservedEvent| true;
    let bash = list_session_window_scan(
        &pool, "sess-scan", None, Some(&["Bash".into()]), &any, None, None, 100,
    ).await.unwrap();
    assert_eq!(bash.len(), 10);
    assert!(bash.iter().all(|e| e.tool_name.as_deref() == Some("Bash")));
    // (5) count
    let cnt = count_session_scan(&pool, "sess-scan", Some(&["user_message".into()]), None, &deploy)
        .await.unwrap();
    assert_eq!(cnt, 10);
}
```

`seed_scan_session`은 이 테스트용 신규 헬퍼: `tests/session_events_kind_filter.rs`의 `seed_pool` 삽입 패턴(raw insert_dedup + ObservedEvent insert)을 축약 이식해 user_message payload `{"content":"deploy N"|"chat N"}`, tool_call `tool_name` Bash/Edit 교차, metric_sample을 초 간격 타임스탬프로 60행 삽입한다.

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --lib db::repo_observed 2>&1 | tail -5`
Expected: 컴파일 실패 — `cannot find function list_session_window_scan`.

- [ ] **Step 3: 구현** — `list_session_window_kinds`의 4-arm SQL 조립을 일반화:

```rust
const SCAN_CHUNK: i64 = 1000;

fn scan_sql(
    sql_kinds: Option<&[String]>,
    sql_tools: Option<&[String]>,
    before: bool,
    after: bool,
    desc: bool,
) -> String {
    let mut sql = String::from("SELECT * FROM observed_event WHERE session_id = ?");
    if let Some(ks) = sql_kinds {
        let ph = (0..ks.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        sql.push_str(&format!(" AND kind IN ({ph})"));
    }
    if let Some(ts) = sql_tools {
        let ph = (0..ts.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        sql.push_str(&format!(" AND tool_name IN ({ph})"));
    }
    if after {
        sql.push_str(" AND (observed_at > ? OR (observed_at = ? AND event_id > ?))");
    }
    if before {
        sql.push_str(" AND (observed_at < ? OR (observed_at = ? AND event_id < ?))");
    }
    sql.push_str(if desc {
        " ORDER BY observed_at DESC, event_id DESC LIMIT ?"
    } else {
        " ORDER BY observed_at ASC, event_id ASC LIMIT ?"
    });
    sql
}

pub async fn list_session_window_scan(
    pool: &SqlitePool,
    session_id: &str,
    sql_kinds: Option<&[String]>,
    sql_tools: Option<&[String]>,
    pred: &dyn Fn(&ObservedEvent) -> bool,
    before: Option<&Cursor>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<ObservedEvent>> {
    let limit = limit.clamp(1, 1000);
    // 방향: after-only는 ASC 전진, 그 외(무커서·before·양쪽)는 기존 창 계약과
    // 동일 — before/무커서는 최신 앵커 DESC, 양쪽은 ASC(상한 = before).
    let desc = after.is_none();
    let mut matched: Vec<ObservedEvent> = Vec::new();
    // 스캔 재개 커서: DESC면 상한(before 자리), ASC면 하한(after 자리)을 전진.
    let mut resume: Option<Cursor> = None;
    loop {
        let eff_before = if desc { resume.as_ref().or(before) } else { before };
        let eff_after = if desc { after } else { resume.as_ref().or(after) };
        let sql = scan_sql(sql_kinds, sql_tools, eff_before.is_some(), eff_after.is_some(), desc);
        let mut q = sqlx::query(&sql).bind(session_id);
        if let Some(ks) = sql_kinds { for k in ks { q = q.bind(k); } }
        if let Some(ts) = sql_tools { for t in ts { q = q.bind(t); } }
        if let Some(a) = eff_after {
            let ts = a.observed_at.to_rfc3339();
            q = q.bind(ts.clone()).bind(ts).bind(a.event_id.clone());
        }
        if let Some(b) = eff_before {
            let ts = b.observed_at.to_rfc3339();
            q = q.bind(ts.clone()).bind(ts).bind(b.event_id.clone());
        }
        let rows = q.bind(SCAN_CHUNK).fetch_all(pool).await?;
        let chunk_len = rows.len() as i64;
        let events: Vec<ObservedEvent> = rows.into_iter().map(row_to_observed).collect();
        if let Some(last) = events.last() {
            resume = Some(Cursor { observed_at: last.observed_at, event_id: last.event_id.clone() });
        }
        for e in events {
            if pred(&e) {
                matched.push(e);
                if matched.len() as i64 >= limit { break; }
            }
        }
        if matched.len() as i64 >= limit || chunk_len < SCAN_CHUNK {
            break;
        }
    }
    if desc {
        matched.reverse(); // 와이어는 항상 ASC
    }
    Ok(matched)
}

pub async fn count_session_scan(
    pool: &SqlitePool,
    session_id: &str,
    sql_kinds: Option<&[String]>,
    sql_tools: Option<&[String]>,
    pred: &dyn Fn(&ObservedEvent) -> bool,
) -> Result<i64> {
    let mut count = 0i64;
    let mut resume: Option<Cursor> = None;
    loop {
        let sql = scan_sql(sql_kinds, sql_tools, false, resume.is_some(), false);
        let mut q = sqlx::query(&sql).bind(session_id);
        if let Some(ks) = sql_kinds { for k in ks { q = q.bind(k); } }
        if let Some(ts) = sql_tools { for t in ts { q = q.bind(t); } }
        if let Some(a) = &resume {
            let ts = a.observed_at.to_rfc3339();
            q = q.bind(ts.clone()).bind(ts).bind(a.event_id.clone());
        }
        let rows = q.bind(SCAN_CHUNK).fetch_all(pool).await?;
        let chunk_len = rows.len() as i64;
        let events: Vec<ObservedEvent> = rows.into_iter().map(row_to_observed).collect();
        if let Some(last) = events.last() {
            resume = Some(Cursor { observed_at: last.observed_at, event_id: last.event_id.clone() });
        }
        count += events.iter().filter(|e| pred(e)).count() as i64;
        if chunk_len < SCAN_CHUNK { break; }
    }
    Ok(count)
}
```

- [ ] **Step 4: 초록 확인**

Run: `cargo test --lib db::repo_observed 2>&1 | tail -5`
Expected: `0 failed`.

- [ ] **Step 5: 커밋**

```bash
git add src/db/repo_observed.rs
git commit -m "feat(db): 커서 청크 스캔 창 — SQL 푸시다운 + Rust 술어 + matched count"
```

---

### Task 5: `session_events` 배선 — 필터 경로·400 가드·`matched_count`

**Files:**
- Modify: `src/api/routes.rs` (EventsQuery 479~498행, session_events 503~699행)
- Modify: `src/api/dto.rs` (SessionEventsResponse 106~111행)
- Create: `tests/session_events_filter.rs`

**Interfaces:**
- Consumes: Task 2 `EventFilter`/`RawFilterParams`/`FilterCtx`, Task 3 repo 함수 2종, Task 4 스캔 함수 2종.
- Produces: `EventsQuery`에 `pub role/origin/error/signal/verification/tool/model/q: Option<String>` 8필드. `SessionEventsResponse`에 `#[serde(skip_serializing_if = "Option::is_none")] pub matched_count: Option<i64>`. 필터 활성 시 tip 규칙: 무커서 창이거나 `after` 전진이 limit 미만 반환이면 `next_cursor: null`(기존 kind 규칙 일반화, 650~663행).

- [ ] **Step 1: 실패 테스트 작성** — `tests/session_events_filter.rs` 생성. `tests/session_events_kind_filter.rs`의 seed·setup 패턴을 이식하되 풍부한 세션을 심는다:

```rust
//! §1.2 (docs/specs/2026-07-04-session-detail-improvements.md) — events 4축 필터.
//! 잠그는 계약: 축별 정확성 / 축 AND·CSV OR / 커서 페이징 결합(경계 누락·중복
//! 없음) / matched_count / around×필터 400 / 미지 값 400.

use axum_test::TestServer;
// (use 블록은 tests/session_events_kind_filter.rs와 동일 + repo_signal, repo_verification_run)

const SESS: &str = "sess-filter";

/// 시드(모두 초 간격, i = 0..):
///  - user_message ×8: 4건 human("deploy i" 2건 + "chat i" 2건), 2건 command
///    ("<command-name>/model</command-name>"), 2건 notification("<task-notification>…")
///  - assistant_message ×4: model "claude-fable-5" 2건 / "claude-haiku-4-5-20251001" 2건
///  - tool_call ×6: Bash 3, Edit 3 (tool_name 컬럼+payload 동시 설정)
///  - tool_result ×6: is_error true 2 / false 4, content "thread panicked" 1건 포함
///  - metric_sample ×6
///  - signal 1행: evidence_refs = 첫 tool_call·둘째 tool_result의 event_id
///  - verification_run 2행: trigger=마지막 Bash tool_call, status failed →
///    5분 뒤 passed (최신 승리 계약은 repo 단위테스트가 잠금 — 여기선 passed로 조회)
async fn seed_pool() -> SqlitePool {
    // pool·raw_event 삽입 보일러플레이트는 tests/session_events_kind_filter.rs
    // seed_pool(25~74행)에서 그대로 가져온다. 이벤트 생성만 아래 스펙으로 교체:
    let specs: Vec<(EventKind, serde_json::Value, Option<&str>)> = vec![
        // (kind, payload, tool_name 컬럼)
        (EventKind::UserMessage, json!({"content": "deploy the fix"}), None),
        (EventKind::UserMessage, json!({"content": "deploy again"}), None),
        (EventKind::UserMessage, json!({"content": "chat one"}), None),
        (EventKind::UserMessage, json!({"content": "chat two"}), None),
        (EventKind::UserMessage, json!({"content": "<command-name>/model</command-name>"}), None),
        (EventKind::UserMessage, json!({"content": "<command-name>/help</command-name>"}), None),
        (EventKind::UserMessage, json!({"content": "<task-notification>done A</task-notification>"}), None),
        (EventKind::UserMessage, json!({"content": "<task-notification>done B</task-notification>"}), None),
        (EventKind::AssistantMessage, json!({"text": "working", "model": "claude-fable-5"}), None),
        (EventKind::AssistantMessage, json!({"text": "done", "model": "claude-fable-5"}), None),
        (EventKind::AssistantMessage, json!({"text": "hm", "model": "claude-haiku-4-5-20251001"}), None),
        (EventKind::AssistantMessage, json!({"text": "ok", "model": "claude-haiku-4-5-20251001"}), None),
        (EventKind::ToolCall, json!({"tool_name": "Bash", "input": {"command": "cargo test"}}), Some("Bash")),
        (EventKind::ToolCall, json!({"tool_name": "Bash", "input": {"command": "ls"}}), Some("Bash")),
        (EventKind::ToolCall, json!({"tool_name": "Bash", "input": {"command": "cargo build"}}), Some("Bash")),
        (EventKind::ToolCall, json!({"tool_name": "Edit", "input": {"file_path": "a.rs"}}), Some("Edit")),
        (EventKind::ToolCall, json!({"tool_name": "Edit", "input": {"file_path": "b.rs"}}), Some("Edit")),
        (EventKind::ToolCall, json!({"tool_name": "Edit", "input": {"file_path": "c.rs"}}), Some("Edit")),
        (EventKind::ToolResult, json!({"tool_result": {"is_error": true, "content": "thread panicked"}}), None),
        (EventKind::ToolResult, json!({"tool_result": {"is_error": true, "content": "exit 1"}}), None),
        (EventKind::ToolResult, json!({"tool_result": {"is_error": false, "content": "ok"}}), None),
        (EventKind::ToolResult, json!({"tool_result": {"is_error": false, "content": "ok"}}), None),
        (EventKind::ToolResult, json!({"tool_result": {"is_error": false, "content": "ok"}}), None),
        (EventKind::ToolResult, json!({"tool_result": {"is_error": false, "content": "ok"}}), None),
    ];
    // + metric_sample 6행(payload json!({})). 각 행 event_id = format!("01K{i:023}"),
    // observed_at = base + i초, tool_name 컬럼은 스펙 3열 값으로 설정.
    // 삽입 후:
    //   repo_signal 테이블에 evidence_refs = json!([specs[12]의 event_id,
    //     {"event_id": specs[19]의 event_id}]) 1행 (Task 3 테스트의 insert_signal_row 이식)
    //   verification_run 2행: trigger = specs[14]의 event_id(마지막 Bash),
    //     ("failed", base+100s) → ("passed", base+400s) (Task 3의 insert_run 이식)
    /* … */
}

async fn get(server: &TestServer, qs: &str) -> serde_json::Value {
    server.get(&format!("/v1/sessions/{SESS}/events{qs}")).await.json::<serde_json::Value>()
}

#[tokio::test]
async fn filter_axes_and_matched_count() {
    let server = setup().await;
    // origin=human → human user_message 4건만
    let v = get(&server, "?origin=human").await;
    assert_eq!(v["data"]["events"].as_array().unwrap().len(), 4);
    assert_eq!(v["data"]["matched_count"], serde_json::json!(4));
    // origin CSV OR
    let v = get(&server, "?origin=command,notification").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(4));
    // error=true
    let v = get(&server, "?error=true").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(2));
    // signal=true → evidence 2건
    let v = get(&server, "?signal=true").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(2));
    // verification=passed → trigger tool_call 1건
    let v = get(&server, "?verification=passed").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(1));
    let v = get(&server, "?verification=failed").await;
    assert_eq!(v["data"]["matched_count"], serde_json::json!(0));
    // tool·model·q·role
    assert_eq!(get(&server, "?tool=Bash").await["data"]["matched_count"], serde_json::json!(3));
    assert_eq!(get(&server, "?model=claude-fable-5").await["data"]["matched_count"], serde_json::json!(2));
    assert_eq!(get(&server, "?q=PANICKED").await["data"]["matched_count"], serde_json::json!(1));
    assert_eq!(get(&server, "?role=assistant").await["data"]["matched_count"], serde_json::json!(4));
    // AND 조합: q=deploy && origin=human → 2건
    assert_eq!(get(&server, "?q=deploy&origin=human").await["data"]["matched_count"], serde_json::json!(2));
    // 필터 없으면 matched_count 자체가 없다
    let v = get(&server, "").await;
    assert!(v["data"].get("matched_count").is_none());
}

#[tokio::test]
async fn filtered_pagination_no_gap_no_dup_and_tip() {
    let server = setup().await;
    // user_message 8건을 limit=3으로 뒤로 페이징: 3+3+2, 합집합 8, 교집합 0
    let p1 = get(&server, "?role=user&limit=3").await;
    let e1 = p1["data"]["events"].as_array().unwrap().clone();
    assert_eq!(e1.len(), 3);
    assert!(p1["data"]["next_cursor"].is_null(), "무커서 필터 창은 tip");
    let prev = p1["data"]["prev_cursor"].as_str().unwrap().to_string();
    let p2 = get(&server, &format!("?role=user&limit=3&before={}", urlencoding::encode(&prev))).await;
    let e2 = p2["data"]["events"].as_array().unwrap().clone();
    assert_eq!(e2.len(), 3);
    let prev2 = p2["data"]["prev_cursor"].as_str().unwrap().to_string();
    let p3 = get(&server, &format!("?role=user&limit=3&before={}", urlencoding::encode(&prev2))).await;
    let e3 = p3["data"]["events"].as_array().unwrap().clone();
    assert_eq!(e3.len(), 2);
    let mut ids: Vec<String> = [&e1, &e2, &e3].into_iter().flatten()
        .map(|e| e["event_id"].as_str().unwrap().to_string()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, 8, "no dup, no gap");
    // after 전진이 limit 미만 → tip(null)
    let tail = e1.last().unwrap();
    let after = format!("{}|{}", tail["observed_at"].as_str().unwrap(), tail["event_id"].as_str().unwrap());
    let p4 = get(&server, &format!("?role=user&limit=5&after={}", urlencoding::encode(&after))).await;
    assert!(p4["data"]["next_cursor"].is_null());
}

#[tokio::test]
async fn filter_param_errors() {
    let server = setup().await;
    // around×필터 → 400 (기존 kind 규칙과 동일 문구 계열)
    let r = server.get(&format!("/v1/sessions/{SESS}/events?origin=human&around=X")).await;
    r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    // 미지 origin/role/verification 값 → 400
    for qs in ["?origin=alien", "?role=bot", "?verification=flaky", "?error=yes"] {
        let r = server.get(&format!("/v1/sessions/{SESS}/events{qs}")).await;
        r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
}
```

`urlencoding`이 dev-dependency에 없으면 커서의 `|`·`+`만 문제이므로 `str::replace('|', "%7C").replace('+', "%2B")` 헬퍼로 대체한다(새 dep 추가 금지).

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --test session_events_filter 2>&1 | tail -10`
Expected: 컴파일 실패(EventsQuery에 role 필드 없음 → 시드 전에 400 아님, deny_unknown_fields로 400은 나지만 matched_count 부재로 assertion 실패) — 어느 쪽이든 red.

- [ ] **Step 3: 구현** — routes.rs:
  1. `EventsQuery`에 8필드 추가(각각 `pub xxx: Option<String>`, doc 주석에 §1.2 표 참조 명기).
  2. `session_events` 초입의 kind 인라인 검증 블록(530~557행)을 제거하고 대체:

```rust
    use crate::insight::event_filter::{EventFilter, FilterCtx, RawFilterParams};
    let filter = EventFilter::from_params(&RawFilterParams {
        kind: q.kind.as_deref(),
        role: q.role.as_deref(),
        origin: q.origin.as_deref(),
        error: q.error.as_deref(),
        signal: q.signal.as_deref(),
        verification: q.verification.as_deref(),
        tool: q.tool.as_deref(),
        model: q.model.as_deref(),
        q: q.q.as_deref(),
    })
    .map_err(|detail| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"type":"about:blank","title":"INVALID_FILTER","detail": detail})),
        )
    })?;
    if filter.is_some()
        && (q.around.is_some() || q.tool_use_id.is_some() || q.request_id.is_some())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "about:blank",
                "title": "INVALID_PARAMS",
                "detail": "filter params cannot be combined with around / tool_use_id / request_id",
            })),
        ));
    }
```

  단, 기존 테스트 `session_events_kind_filter.rs`는 `INVALID_KIND` title을 잠근다 — 확인 후 필요하면 `from_params`의 kind 오류만 `INVALID_KIND`로 매핑(오류 문자열이 `unknown event kind:`로 시작하면 title을 `INVALID_KIND`, 아니면 `INVALID_FILTER`).
  3. 창 조회 분기(614~638행): `kind_filter` 분기를 `filter` 분기로 교체 —

```rust
    let mut matched_count: Option<i64> = None;
    let evs = if let Some(around_id) = q.around.as_deref() {
        /* 기존 around 분기 그대로 */
    } else if let Some(f) = &filter {
        let ctx = if f.needs_ctx() {
            FilterCtx {
                signal_evidence: repo_signal::evidence_event_ids(&pool, &id).await.expect("db"),
                verification_by_trigger: repo_verification_run::status_by_trigger(&pool, &id)
                    .await
                    .expect("db"),
            }
        } else {
            FilterCtx::default()
        };
        let pred = |e: &crate::model::observed::ObservedEvent| f.matches(e, &ctx);
        // kind·tool은 SQL 푸시다운, matches()에도 남아 있어 이중 안전.
        let before = parse_cursor(q.before.as_deref())?;
        let after = parse_cursor(q.after.as_deref())?;
        matched_count = Some(
            repo_observed::count_session_scan(
                &pool, &id, f.kinds.as_deref(), f.tools.as_deref(), &pred,
            )
            .await
            .expect("db"),
        );
        repo_observed::list_session_window_scan(
            &pool, &id, f.kinds.as_deref(), f.tools.as_deref(), &pred,
            before.as_ref(), after.as_ref(), limit,
        )
        .await
        .expect("db")
    } else {
        /* 기존 unfiltered list_session_window 분기 그대로 */
    };
```

  4. tip 규칙(650~663행): `kind_filter.is_some()` 조건을 `filter.is_some()`으로 치환(로직 동일).
  5. 응답 조립: `SessionEventsResponse { events, prev_cursor, next_cursor, matched_count }`. dto.rs에 필드 추가 후 correlated 분기(585~590행)와 기타 생성처는 `matched_count: None`.
  6. `parse_cursor`는 filter 분기보다 위에 있어야 한다(현재 fn 내 정의라 이동 불필요 — 호출 순서만 주의).

- [ ] **Step 4: 초록 + 기존 계약 회귀 확인**

Run: `cargo test --test session_events_filter --test session_events_kind_filter 2>&1 | tail -8`
Expected: 두 파일 모두 `0 failed`. **kind_filter 테스트는 무수정 통과가 계약** — 수정이 필요해지면 구현이 계약을 깬 것.

- [ ] **Step 5: 전체 회귀 + 커밋**

Run: `cargo test 2>&1 | grep -E "FAILED|failed" | tail -5` → 모든 스위트 `0 failed` 확인.

```bash
git add src/api/routes.rs src/api/dto.rs tests/session_events_filter.rs
git commit -m "feat(api): events 4축 서버 필터 — AND/OR·matched_count·around 400 (§1.2)"
```

---

### Task 6: kind 전용 repo 창 제거 (경로 통합 마무리)

**Files:**
- Modify: `src/db/repo_observed.rs` (`list_session_window_kinds` 885~954행 삭제)

- [ ] **Step 1: 참조 0 확인**

Run: `grep -rn "list_session_window_kinds" src/ tests/`
Expected: 정의 1곳만(Task 5에서 routes.rs 호출 제거 완료 상태). 다른 호출이 남았으면 이 태스크 중단하고 Task 5 재검토.

- [ ] **Step 2: 함수 삭제 후 전체 테스트**

Run: `cargo test 2>&1 | grep -cE "0 failed" ; cargo clippy --all-targets -- -D warnings 2>&1 | tail -3`
Expected: 전 스위트 통과, clippy 경고 0 (dead_code 경고가 삭제 검증).

- [ ] **Step 3: 커밋**

```bash
git add src/db/repo_observed.rs
git commit -m "refactor(db): list_session_window_kinds 제거 — 필터 스캔 경로로 통합"
```

---

### Task 7: 프론트 필터 상태 — `FilterState`·URL 왕복·클라이언트 파라미터

**Files:**
- Create: `webui/src/components/replay/stream/filterState.ts`
- Create: `webui/src/components/replay/stream/__tests__/filterState.test.ts`
- Modify: `webui/src/api/types.ts` (`SessionEventsResponse`에 `matched_count?: number`)
- Modify: `webui/src/api/client.ts` (getSessionEvents 59~73행)

**Interfaces:**
- Produces:

```ts
// filterState.ts
export interface FilterState {
  kinds: string[]; roles: string[]; origins: string[];
  error: boolean; signal: boolean; verifications: string[];
  tools: string[]; models: string[]; q: string;
}
export const EMPTY_FILTER: FilterState;
export function isFilterActive(f: FilterState): boolean;
/** 서버 쿼리 파라미터(스펙 §1.2 이름). 비활성 축은 키 자체를 생략. */
export interface EventFilterParams {
  kind?: string; role?: string; origin?: string; error?: 'true'; signal?: 'true';
  verification?: string; tool?: string; model?: string; q?: string;
}
export function toEventFilterParams(f: FilterState): EventFilterParams;
/** URL 동기화 — 접두 f_ (react-router searchParams). 왕복 무손실. */
export function filterToSearch(f: FilterState, sp: URLSearchParams): void;   // f_* 키만 갱신/삭제
export function filterFromSearch(sp: URLSearchParams): FilterState;
/** 필터 정체성 비교(윈도우 리셋 트리거) — 직렬화 키 */
export function filterKey(f: FilterState): string;
```

- client.ts: `getSessionEvents(id, opts?: { before?; after?; around?; limit?; filter?: EventFilterParams })` — 기존 `kind?: string` 옵션은 `filter.kind`로 흡수(기존 호출처는 useSessionWindow뿐이므로 grep으로 확인 후 제거).

- [ ] **Step 1: 실패 테스트 작성** — `filterState.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  EMPTY_FILTER, filterFromSearch, filterKey, filterToSearch,
  isFilterActive, toEventFilterParams, type FilterState,
} from '../filterState';

const sample: FilterState = {
  ...EMPTY_FILTER,
  kinds: ['tool_call', 'tool_result'],
  origins: ['human'],
  error: true,
  q: 'panic',
};

describe('filterState', () => {
  it('EMPTY_FILTER is inactive; any axis activates', () => {
    expect(isFilterActive(EMPTY_FILTER)).toBe(false);
    expect(isFilterActive(sample)).toBe(true);
    expect(isFilterActive({ ...EMPTY_FILTER, signal: true })).toBe(true);
  });

  it('toEventFilterParams emits spec §1.2 param names, omitting inactive axes', () => {
    expect(toEventFilterParams(sample)).toEqual({
      kind: 'tool_call,tool_result', origin: 'human', error: 'true', q: 'panic',
    });
    expect(toEventFilterParams(EMPTY_FILTER)).toEqual({});
  });

  it('URL round-trip is lossless and prunes stale f_* keys', () => {
    const sp = new URLSearchParams('selected=EV1&f_model=claude-fable-5');
    filterToSearch(sample, sp);
    expect(sp.get('selected')).toBe('EV1');       // 비필터 키 보존
    expect(sp.get('f_model')).toBeNull();          // 비활성 축 제거
    expect(filterFromSearch(sp)).toEqual(sample);
  });

  it('filterKey changes iff filter content changes', () => {
    expect(filterKey(sample)).not.toBe(filterKey(EMPTY_FILTER));
    expect(filterKey({ ...sample })).toBe(filterKey(sample));
  });
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/filterState.test.ts 2>&1 | tail -5`
Expected: FAIL — `Cannot find module '../filterState'`.

- [ ] **Step 3: 구현**

```ts
// filterState.ts — 스펙 §1.2/§1.4. 축끼리 AND, 축 내 CSV OR. URL 키는 f_ 접두.
export interface FilterState { /* Interfaces 블록 그대로 */ }

export const EMPTY_FILTER: FilterState = {
  kinds: [], roles: [], origins: [], error: false, signal: false,
  verifications: [], tools: [], models: [], q: '',
};

export function isFilterActive(f: FilterState): boolean {
  return (
    f.kinds.length > 0 || f.roles.length > 0 || f.origins.length > 0 ||
    f.error || f.signal || f.verifications.length > 0 ||
    f.tools.length > 0 || f.models.length > 0 || f.q.trim() !== ''
  );
}

export interface EventFilterParams { /* Interfaces 블록 그대로 */ }

export function toEventFilterParams(f: FilterState): EventFilterParams {
  const p: EventFilterParams = {};
  if (f.kinds.length) p.kind = f.kinds.join(',');
  if (f.roles.length) p.role = f.roles.join(',');
  if (f.origins.length) p.origin = f.origins.join(',');
  if (f.error) p.error = 'true';
  if (f.signal) p.signal = 'true';
  if (f.verifications.length) p.verification = f.verifications.join(',');
  if (f.tools.length) p.tool = f.tools.join(',');
  if (f.models.length) p.model = f.models.join(',');
  if (f.q.trim()) p.q = f.q.trim();
  return p;
}

const LIST_KEYS = [
  ['f_kind', 'kinds'], ['f_role', 'roles'], ['f_origin', 'origins'],
  ['f_verification', 'verifications'], ['f_tool', 'tools'], ['f_model', 'models'],
] as const;

export function filterToSearch(f: FilterState, sp: URLSearchParams): void {
  for (const [key, prop] of LIST_KEYS) {
    const v = f[prop];
    if (v.length) sp.set(key, v.join(','));
    else sp.delete(key);
  }
  if (f.error) sp.set('f_error', 'true'); else sp.delete('f_error');
  if (f.signal) sp.set('f_signal', 'true'); else sp.delete('f_signal');
  if (f.q.trim()) sp.set('f_q', f.q.trim()); else sp.delete('f_q');
}

export function filterFromSearch(sp: URLSearchParams): FilterState {
  const list = (k: string) => sp.get(k)?.split(',').map((s) => s.trim()).filter(Boolean) ?? [];
  return {
    kinds: list('f_kind'), roles: list('f_role'), origins: list('f_origin'),
    error: sp.get('f_error') === 'true', signal: sp.get('f_signal') === 'true',
    verifications: list('f_verification'), tools: list('f_tool'), models: list('f_model'),
    q: sp.get('f_q') ?? '',
  };
}

export function filterKey(f: FilterState): string {
  return JSON.stringify(toEventFilterParams(f));
}
```

client.ts — `opts.kind` 제거, `filter` 추가:

```ts
export function getSessionEvents(
  id: string,
  opts?: { before?: string; after?: string; around?: string; limit?: number; filter?: EventFilterParams },
): Promise<SessionEventsResponse> {
  const params = new URLSearchParams();
  if (opts?.before) params.set('before', opts.before);
  if (opts?.after) params.set('after', opts.after);
  if (opts?.around) params.set('around', opts.around);
  if (opts?.limit !== undefined) params.set('limit', String(opts.limit));
  for (const [k, v] of Object.entries(opts?.filter ?? {})) {
    if (v !== undefined) params.set(k, v);
  }
  const qs = params.toString();
  const path = `/v1/sessions/${encodeURIComponent(id)}/events` + (qs ? `?${qs}` : '');
  return jsonGet<SessionEventsResponse>(path);
}
```

types.ts의 `SessionEventsResponse`에 `matched_count?: number` 추가. `grep -rn "kind:" webui/src --include="*.ts*" | grep getSessionEvents`로 기존 kind 옵션 호출처 부재 확인.

- [ ] **Step 4: 초록 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/filterState.test.ts 2>&1 | tail -5`
Expected: `4 passed`. 이어서 `npx tsc --noEmit`로 타입 회귀 없음 확인.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/components/replay/stream/filterState.ts \
  webui/src/components/replay/stream/__tests__/filterState.test.ts \
  webui/src/api/client.ts webui/src/api/types.ts
git commit -m "feat(webui): FilterState — URL 왕복·서버 파라미터 직렬화"
```

---

### Task 8: `useSessionWindow` 필터 결합 — 전 fetch 적용·변경 리셋·matchedCount

**Files:**
- Modify: `webui/src/hooks/useSessionWindow.ts`
- Modify: `webui/src/hooks/__tests__/useSessionWindow.test.tsx`

**Interfaces:**
- Consumes: Task 7 `EventFilterParams`.
- Produces: `UseSessionWindowOpts.filter?: EventFilterParams | null`(활성 필터; null/undefined = 비활성), `UseSessionWindowOpts.filterKey?: string`(정체성 — 변경 시 버퍼 리셋·tail 재로드), `UseSessionWindowResult.matchedCount: number | null`. `loadTail`·`loadOlder`·`loadNewer`가 `filter`를 전달, `loadAround`는 전달하지 않음(§1.2 around×필터 미지원 — 호출 전 필터 해제는 페이지 책임).

- [ ] **Step 1: 실패 테스트 작성** — 기존 `useSessionWindow.test.tsx`의 mock 패턴(getSessionEvents mock)을 재사용해 추가:

```tsx
it('passes filter params to tail/older/newer fetches and resets on filterKey change', async () => {
  const filter = { origin: 'human', q: 'deploy' } as const;
  const { result, rerender } = renderHook(
    ({ fk }) => useSessionWindow('s1', { filter, filterKey: fk }),
    { initialProps: { fk: 'A' } },
  );
  await waitFor(() => expect(result.current.loading).toBe('idle'));
  // 초기 tail fetch에 필터 파라미터 포함
  expect(mockGetSessionEvents).toHaveBeenLastCalledWith(
    's1', expect.objectContaining({ filter: expect.objectContaining({ origin: 'human' }) }),
  );
  await act(() => result.current.loadOlder());
  expect(mockGetSessionEvents).toHaveBeenLastCalledWith(
    's1', expect.objectContaining({ before: expect.any(String), filter: expect.objectContaining({ q: 'deploy' }) }),
  );
  // filterKey 변경 → 버퍼 리셋(재-initial)
  const callsBefore = mockGetSessionEvents.mock.calls.length;
  rerender({ fk: 'B' });
  await waitFor(() => expect(mockGetSessionEvents.mock.calls.length).toBeGreaterThan(callsBefore));
});

it('exposes matched_count from the newest response', async () => {
  mockGetSessionEvents.mockResolvedValueOnce({
    events: [], prev_cursor: null, next_cursor: null, matched_count: 42,
  });
  const { result } = renderHook(() => useSessionWindow('s1', { filter: { q: 'x' }, filterKey: 'k' }));
  await waitFor(() => expect(result.current.matchedCount).toBe(42));
});
```

mock 헬퍼 이름(`mockGetSessionEvents` 등)은 기존 테스트 파일의 실제 이름에 맞춘다.

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/hooks/__tests__/useSessionWindow.test.tsx 2>&1 | tail -6`
Expected: FAIL — filter 옵션 무시(파라미터 미포함) + matchedCount undefined.

- [ ] **Step 3: 구현** — useSessionWindow.ts 수정 요지:
  1. opts에 `filter`, `filterKey` 추가; `const filter = opts.filter ?? null;`.
  2. `const [matchedCount, setMatchedCount] = useState<number | null>(null);` — 모든 응답 처리에서 `setMatchedCount(resp.matched_count ?? null)` (tail/older/newer 공통; loadAround는 null 유지).
  3. `loadTail`(136행) / `loadOlder`(181행) / `loadNewer`(206행)의 `getSessionEvents` 호출에 `...(filter ? { filter } : {})` 스프레드 추가, dep 배열에 `filter` 추가.
  4. 리셋: `doInitial`의 dep에 `opts.filterKey`가 물리도록 `const filterKey = opts.filterKey ?? '';`를 `loadTail`/`doInitial` deps에 추가 — filterKey 변경 시 기존 `useEffect(() => { void doInitial(); }, [doInitial])`(172행)가 재발화해 버퍼가 교체된다. `loadTail`은 시작 시 `setEvents([])`를 하지 않으므로(교체는 응답에서) 추가 코드는 deps 연결이 전부.
  5. 반환 객체에 `matchedCount` 추가.

- [ ] **Step 4: 초록 확인**

Run: `cd webui && npx vitest run src/hooks/__tests__/useSessionWindow.test.tsx 2>&1 | tail -5`
Expected: 기존 + 신규 전부 pass, `0 failed`.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/hooks/useSessionWindow.ts webui/src/hooks/__tests__/useSessionWindow.test.tsx
git commit -m "feat(webui): useSessionWindow 필터 파라미터·리셋·matchedCount"
```

---

### Task 9: flat 모드 — 그룹핑 비활성 + 출처 배지

**Files:**
- Modify: `webui/src/components/replay/stream/streamModel.ts` (`buildStreamModel` 시그니처)
- Modify: `webui/src/components/replay/stream/MessageCard.tsx`, `ActivityStack.tsx`, `ConversationStream.tsx`
- Modify: `webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts`
- Modify: `webui/src/i18n/catalog/en.ts`, `webui/src/i18n/catalog/ko.ts`

**Interfaces:**
- Produces: `buildStreamModel(events, metricsByReq, tasks, opts?: { flat?: boolean })`. flat=true면 sidechain-group/batch-group/workflow-group/scaffold-group을 만들지 않고 모든 이벤트를 메인 스파인 규칙으로 평면 분류(message/activity-run/thinking/종료카드 유지 — §1.4). `MessageItem`·`ActivityEvent`는 기존 `sidechain` 정보를 이미 보유 — 카드 렌더에 `flatMode?: boolean` prop을 내려 `is_sidechain` 항목에 배지를 붙인다.
- i18n 키: `stream.flatSidechainBadge` = en `"⑂ inside subagent"` / ko `"⑂ 서브에이전트 내부"`.

- [ ] **Step 1: 실패 테스트 작성** — `buildStreamModel.test.ts`에 추가(기존 fixture 헬퍼 재사용 — 이 파일에는 sidechain 그룹을 만드는 케이스가 이미 있다; 그 시드를 flat 옵션으로 재호출):

```ts
it('flat mode: no group items, sidechain events render as flat cards', () => {
  // buildStreamModel.test.ts에는 sidechain-group을 만드는 기존 케이스가 있다
  // (`is_sidechain: true` 이벤트 시드) — 그 케이스의 이벤트 배열 생성부를
  // 지역 헬퍼로 추출해 두 호출이 같은 시드를 쓰게 한다. 기존 케이스에서 그대로
  // 복사한 최소 형태(필드명은 그 파일의 ObservedEventDto 시드 관례를 따른다):
  const events = [
    dtoEvent({ event_id: 'E1', kind: 'user_message', payload: { content: 'go' } }),
    dtoEvent({ event_id: 'E2', kind: 'tool_call', tool_name: 'Agent',
      tool_use_id: 'T1', payload: { tool_name: 'Agent', input: {} } }),
    dtoEvent({ event_id: 'E3', kind: 'user_message', is_sidechain: true,
      agent_id: 'a1', payload: { content: 'sub task' } }),
    dtoEvent({ event_id: 'E4', kind: 'assistant_message', is_sidechain: true,
      agent_id: 'a1', payload: { text: 'sub reply', model: 'claude-fable-5' } }),
    dtoEvent({ event_id: 'E5', kind: 'tool_result', tool_use_id: 'T1',
      payload: { tool_result: { is_error: false, content: 'done' } } }),
  ];
  const grouped = buildStreamModel(events, new Map(), []);
  expect(grouped.some((i) => i.type === 'sidechain-group')).toBe(true);
  const flat = buildStreamModel(events, new Map(), [], { flat: true });
  expect(flat.some((i) =>
    i.type === 'sidechain-group' || i.type === 'batch-group' ||
    i.type === 'workflow-group' || i.type === 'scaffold-group',
  )).toBe(false);
  // 그룹에 갇혀 있던 sidechain 메시지(E3·E4)가 평면 카드로 등장
  const flatMessages = flat.filter((i) => i.type === 'message');
  const groupedTopMessages = grouped.filter((i) => i.type === 'message');
  expect(flatMessages.length).toBeGreaterThan(groupedTopMessages.length);
});
```

(`dtoEvent`는 기존 테스트 파일의 이벤트 시드 헬퍼 — 이름이 다르면 그 파일 관례를 따르고, 없으면 `ObservedEventDto` 필수 필드(observed_at 증가 등)를 채우는 지역 헬퍼로 작성. `type === 'message'` 판별 필드명도 `StreamItem` 유니온의 실제 태그(예: `kind` vs `type`)에 맞춘다 — streamModel.ts 333~344행 확인.)

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/buildStreamModel.test.ts 2>&1 | tail -5`
Expected: FAIL — 4번째 인자 무시로 flat에도 그룹 존재.

- [ ] **Step 3: 구현**
  1. `buildStreamModel(events, metricsByReq, tasks, opts?: { flat?: boolean })` — 내부에서 그룹 조립 단계(사이드체인 수집→SubagentGroup, batch, workflow, scaffold run 묶기)를 `if (!opts?.flat)` 가드로 감싼다. flat일 때 sidechain 이벤트는 메인 스파인과 동일한 classify 경로로 message/activity 카드가 된다(기존 sidechain 분기에서 그룹 대신 straight-through).
  2. `ConversationStream`에 `flatMode?: boolean` prop 추가 → `MessageCard`/`ActivityStack`에 전달.
  3. `MessageCard`: `flatMode && item.sidechain`이면 기존 source-badge 옆에 `<span className="flat-badge">{t('stream.flatSidechainBadge')}</span>`. ActivityStack도 동일 규칙(스택 헤더에 1회).
  4. i18n 두 카탈로그에 키 추가.

- [ ] **Step 4: 초록 확인**

Run: `cd webui && npx vitest run src/components/replay/stream 2>&1 | grep -E "failed|passed" | tail -3`
Expected: 스트림 테스트 전체 `0 failed`(기존 그룹 테스트 무회귀 — flat 기본값 false).

- [ ] **Step 5: 커밋**

```bash
git add webui/src/components/replay/stream/streamModel.ts \
  webui/src/components/replay/stream/MessageCard.tsx \
  webui/src/components/replay/stream/ActivityStack.tsx \
  webui/src/components/replay/stream/ConversationStream.tsx \
  webui/src/components/replay/stream/__tests__/buildStreamModel.test.ts \
  webui/src/i18n/catalog/en.ts webui/src/i18n/catalog/ko.ts
git commit -m "feat(webui): 필터 flat 모드 — 그룹핑 비활성 + 서브에이전트 출처 배지"
```

---

### Task 10: FilterBar 컴포넌트

**Files:**
- Create: `webui/src/components/replay/stream/FilterBar.tsx`
- Create: `webui/src/components/replay/stream/__tests__/FilterBar.test.tsx`
- Modify: `webui/src/i18n/catalog/en.ts`, `webui/src/i18n/catalog/ko.ts`

**Interfaces:**
- Consumes: Task 7 `FilterState`/`EMPTY_FILTER`/`isFilterActive`.
- Produces: `<FilterBar filter={FilterState} onChange={(f: FilterState) => void} matchedCount={number | null} notice={string | null} />`. 내부 구성: 4개 축 드롭다운(kind/origin/결과/도구·모델 — 결과 드롭다운은 error·signal·verification 체크 항목 묶음) + 텍스트 입력(300ms 디바운스로 onChange) + 활성 조건 제거형 칩 + "N건 매칭"(matchedCount null이면 미표시) + 전체 해제 버튼. `notice`는 점프-해제 알림 문구(§1.4 — Task 11이 공급).
- kind 드롭다운 선택지는 스트림에 의미 있는 8종만: `user_message`,`assistant_message`,`thinking`,`tool_call`,`tool_result`,`hook_event`,`system_summary`,`diff_hunk`(repo_observed.rs RENDERED 상수와 동일 집합). origin 7종·verification 3종은 스펙 §1.2 표 그대로 상수 배열.
- i18n 키(en/ko 동시): `filter.title`("Filter"/"필터"), `filter.axis.kind`("Kind"/"종류"), `filter.axis.origin`("Origin"/"출처"), `filter.axis.outcome`("Outcome"/"실행 결과"), `filter.axis.content`("Tool·model·text"/"도구·모델·텍스트"), `filter.outcome.error`("errored tools"/"에러난 도구"), `filter.outcome.signal`("with signals"/"시그널 있음"), `filter.outcome.verification`("verification"/"검증"), `filter.qPlaceholder`("search text…"/"텍스트 검색…"), `filter.matched`("{n} matched"/"{n}건 매칭"), `filter.clearAll`("Clear"/"전체 해제"), `filter.cleared.byJump`("Filter cleared to jump to the event"/"이벤트로 이동하며 필터를 해제했습니다").

- [ ] **Step 1: 실패 테스트 작성** — `FilterBar.test.tsx`(testing-library, 기존 `*.test.tsx` 렌더 관례 재사용):

```tsx
it('renders active chips, matched count, and emits onChange on chip removal', () => {
  const onChange = vi.fn();
  render(
    <FilterBar
      filter={{ ...EMPTY_FILTER, tools: ['Bash'], error: true, q: 'panic' }}
      onChange={onChange}
      matchedCount={7}
      notice={null}
    />,
  );
  expect(screen.getByText(/7건 매칭|7 matched/)).toBeInTheDocument();
  // Bash 칩 제거 → tools 빠진 상태로 onChange
  fireEvent.click(screen.getByRole('button', { name: /Bash/ }));
  expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ tools: [], error: true }));
});

it('debounces text input by 300ms', async () => {
  vi.useFakeTimers();
  const onChange = vi.fn();
  render(<FilterBar filter={EMPTY_FILTER} onChange={onChange} matchedCount={null} notice={null} />);
  fireEvent.change(screen.getByPlaceholderText(/텍스트 검색|search text/), { target: { value: 'oom' } });
  expect(onChange).not.toHaveBeenCalled();
  await act(() => vi.advanceTimersByTime(320));
  expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ q: 'oom' }));
  vi.useRealTimers();
});

it('shows the jump-clear notice when provided', () => {
  render(<FilterBar filter={EMPTY_FILTER} onChange={() => {}} matchedCount={null}
    notice="이벤트로 이동하며 필터를 해제했습니다" />);
  expect(screen.getByRole('status')).toHaveTextContent('필터를 해제했습니다');
});
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/FilterBar.test.tsx 2>&1 | tail -5`
Expected: FAIL — 모듈 없음.

- [ ] **Step 3: 구현** — 드롭다운은 기존 UI 관례를 따른다(프로젝트에 shadcn 계열 컴포넌트가 있으면 재사용 — `webui/src/components/dash/CohortBoundaries.tsx`의 셀렉터 패턴 참조). 골격:

```tsx
export interface FilterBarProps {
  filter: FilterState;
  onChange: (f: FilterState) => void;
  matchedCount: number | null;
  notice: string | null;
}

export function FilterBar({ filter, onChange, matchedCount, notice }: FilterBarProps) {
  const { t } = useI18n(); // 기존 i18n 훅 관례(다른 stream 컴포넌트와 동일하게 import)
  const [qDraft, setQDraft] = useState(filter.q);
  useEffect(() => setQDraft(filter.q), [filter.q]);
  useEffect(() => {
    if (qDraft === filter.q) return;
    const id = setTimeout(() => onChange({ ...filter, q: qDraft }), 300);
    return () => clearTimeout(id);
  }, [qDraft]); // filter/onChange는 의도적으로 제외 — 디바운스 재시작 방지 주석 필수
  /* 축 드롭다운 4개 + 활성 칩 목록 + matchedCount + clearAll + notice(role="status") */
}
```

활성 칩: 각 목록 축 값당 1칩(라벨 = 값 그대로, kind는 snake_case 그대로 — 판정어 금지 원칙상 가공하지 않는다), error/signal은 축 라벨 칩, q는 `q:"…"` 칩. 클릭 시 해당 값 제거한 FilterState로 onChange.

- [ ] **Step 4: 초록 + parity 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/FilterBar.test.tsx src/i18n 2>&1 | grep -E "failed|passed" | tail -3`
Expected: FilterBar 3 pass + i18n parity/tipStyle `0 failed`.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/components/replay/stream/FilterBar.tsx \
  webui/src/components/replay/stream/__tests__/FilterBar.test.tsx \
  webui/src/i18n/catalog/en.ts webui/src/i18n/catalog/ko.ts
git commit -m "feat(webui): FilterBar — 축별 칩·텍스트 검색·매칭 수·해제 알림"
```

---

### Task 11: SessionDetailPage 배선 — URL 동기화·점프 규칙·"새 이벤트 ↓"

**Files:**
- Modify: `webui/src/routes/SessionDetailPage.tsx`
- Modify: `webui/src/components/replay/stream/AutoscrollToggle.tsx` (+ `__tests__/AutoscrollToggle.test.tsx`)
- Modify: `webui/src/components/replay/stream/ConversationStream.tsx` (props 전달)
- Modify: `webui/src/i18n/catalog/en.ts`, `ko.ts` (`stream.newEvents` = en `"new ↓"` / ko `"새 이벤트 ↓"`)

**Interfaces:**
- Consumes: Task 7~10 전부.
- Produces(페이지 내부 배선 — 이후 태스크 없음):
  - `const [searchParams, setSearchParams] = useSearchParams()`(기존 사용 여부 확인 후 재사용) → `filter = filterFromSearch(searchParams)`, 변경은 `filterToSearch` 후 `setSearchParams(sp, { replace: true })`.
  - `useSessionWindow(sessionId, { …, filter: isFilterActive(filter) ? toEventFilterParams(filter) : null, filterKey: filterKey(filter) })`.
  - `buildStreamModel(events, metricsByReq, tasks, { flat: isFilterActive(filter) })` (209~214행 — instructionMarkers 삽입은 flat에서도 유지).
  - **점프 규칙(§1.4)**: 외부발 점프 진입점은 딥링크 around effect(268~283행) 한 곳이다(시그널/분석 점프도 `sel.setSelectedNodeId` → 이 effect로 수렴). 수정: `windowEvents.some(...)`이 false이고 `isFilterActive(filter)`면 — loadAround를 부르기 전에 필터를 해제(`clearFilter()` = EMPTY_FILTER를 URL에 반영)하고 `setJumpNotice(t('filter.cleared.byJump'))`(4초 후 자동 소거) — 필터 해제로 filterKey가 바뀌며 버퍼가 리셋되고, 그 뒤 기존 effect가 loadAround를 수행한다. 버퍼 안에 있으면(필터 매칭 대상) 해제 없이 그대로 스크롤.
  - **대기 배지**: AutoscrollToggle에 `indeterminate?: boolean` prop 추가 — true면 `{newCount} ↓` 대신 `t('stream.newEvents')` 표시(24행 `showCount` 조건에 `|| (indeterminate && newCount > 0)` 반영). ConversationStream이 `filterActive`를 받아 전달.

- [ ] **Step 1: 실패 테스트 작성** — (a) AutoscrollToggle 단위 테스트에 추가:

```tsx
it('indeterminate: shows "새 이벤트 ↓" label instead of a count', () => {
  render(<AutoscrollToggle autoscroll={false} newCount={3} indeterminate
    onEnable={() => {}} onDisable={() => {}} />);
  expect(screen.queryByText('3 ↓')).toBeNull();
  expect(screen.getByText(/새 이벤트 ↓|new ↓/)).toBeInTheDocument();
});
```

(b) 점프 규칙은 순수 함수로 분리해 잠근다 — `filterState.test.ts`에 추가:

```ts
import { jumpNeedsFilterClear } from '../filterState';

it('jumpNeedsFilterClear: clear only when filter active AND target outside buffer', () => {
  // 버퍼 안 = 필터 매칭 대상(필터 창은 매칭 이벤트만 담는다) → 해제 불필요
  expect(jumpNeedsFilterClear(true, true)).toBe(false);
  expect(jumpNeedsFilterClear(true, false)).toBe(true);
  expect(jumpNeedsFilterClear(false, false)).toBe(false);
  expect(jumpNeedsFilterClear(false, true)).toBe(false);
});
```

filterState.ts에 구현(§1.4 — 비매칭 판정은 서버만 가능하므로 "버퍼 밖 = 해제"가 결정론 규칙):

```ts
/** 외부발 점프(§1.4): 필터 활성이고 대상이 로드된(=매칭) 버퍼 밖이면 필터를
 *  해제하고 이동한다. 버퍼 안이면 매칭 대상이므로 해제 없이 스크롤. */
export function jumpNeedsFilterClear(filterActive: boolean, targetInBuffer: boolean): boolean {
  return filterActive && !targetInBuffer;
}
```

- [ ] **Step 2: 빨강 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/AutoscrollToggle.test.tsx 2>&1 | tail -4`
Expected: FAIL — prop 없음/카운트 표시.

- [ ] **Step 3: 구현** — Interfaces 블록의 배선 그대로. SessionDetailPage 요지:

```tsx
const [searchParams, setSearchParams] = useSearchParams();
const filter = useMemo(() => filterFromSearch(searchParams), [searchParams]);
const filterActive = isFilterActive(filter);
const applyFilter = useCallback((f: FilterState) => {
  setSearchParams((sp) => { const next = new URLSearchParams(sp); filterToSearch(f, next); return next; },
    { replace: true });
}, [setSearchParams]);
const [jumpNotice, setJumpNotice] = useState<string | null>(null);
// … useSessionWindow 호출에 filter/filterKey, buildStreamModel에 { flat: filterActive } …
// 딥링크 effect(268행) 내부 — 기존 `windowEvents.some(...)` 검사 결과가
// targetInBuffer. loadAround 진입 직전:
//   if (jumpNeedsFilterClear(filterActive, targetInBuffer)) {
//     applyFilter(EMPTY_FILTER); setJumpNotice(t('filter.cleared.byJump'));
//     setTimeout(() => setJumpNotice(null), 4000); return;
//   }  // 필터 해제 → filterKey 변경 → 버퍼 리셋 후 이 effect 재발화가 loadAround 수행
```

스트림 슬롯 상단에 `<FilterBar filter={filter} onChange={applyFilter} matchedCount={window_.matchedCount} notice={jumpNotice} />` 렌더(383~477행 grid의 `stream` 슬롯 첫 자식). SSE 백필은 추가 작업 없음 — loadNewer/loadTail이 이미 필터 파라미터를 갖는다(Task 8). j/k/e도 streamItems가 이미 필터된 목록이라 무변경.

- [ ] **Step 4: 초록 + 타입 확인**

Run: `cd webui && npx vitest run 2>&1 | grep -E "Tests|failed" | tail -3 && npx tsc --noEmit`
Expected: vitest 전체 `0 failed`, tsc 무오류.

- [ ] **Step 5: 커밋**

```bash
git add webui/src/routes/SessionDetailPage.tsx \
  webui/src/components/replay/stream/AutoscrollToggle.tsx \
  webui/src/components/replay/stream/__tests__/AutoscrollToggle.test.tsx \
  webui/src/components/replay/stream/ConversationStream.tsx \
  webui/src/components/replay/stream/filterState.ts \
  webui/src/components/replay/stream/__tests__/filterState.test.ts \
  webui/src/i18n/catalog/en.ts webui/src/i18n/catalog/ko.ts
git commit -m "feat(webui): 세션 필터 배선 — URL 동기화·점프 시 해제·새 이벤트 배지"
```

---

### Task 12: 브라우저 smoke · 개선 루프 · implementation-notes

**Files:**
- Modify: `docs/implementation-notes.html` (append-only 원장 — 새 앵커 `#session-filtering-2026-07-04`)
- Modify: `docs/notes-index.md` ("WebUI replay·목록" 행 갱신)

- [ ] **Step 1: 스크래치 smoke 스택 기동** (운영 serve :7878은 라이브 CC 세션을 물고 있다 — **절대 재시작 금지**)

```bash
cd /Users/bahamoth/projects/whats-in-my-cc
cd webui && npm run build && cd ..
cargo build
cp "$(ls ~/.local/share/wimcc/*.sqlite 2>/dev/null | head -1)" /tmp/wimcc-smoke.sqlite 2>/dev/null \
  || ./target/debug/wimcc --db-path /tmp/wimcc-smoke.sqlite init-db
./target/debug/wimcc --db-path /tmp/wimcc-smoke.sqlite serve --port 7999 --auto-migrate &
cd webui && WIMCC_PROXY_TARGET=http://127.0.0.1:7999 npx vite --port 5174
```

(`--auto-migrate` 누락 금지 — 2026-07-04 실사고. 스모크 DB에 이벤트가 없으면 `./target/debug/wimcc --db-path /tmp/wimcc-smoke.sqlite ingest --all` 후 재기동.)

- [ ] **Step 2: 브라우저 시각 검증** — `http://localhost:5174`에서 세션 상세 진입 후 확인 목록:
  1. FilterBar 표시, `origin=human` 선택 → 스트림이 사람 메시지만으로 재로드 + "N건 매칭".
  2. `q=cargo` 텍스트 검색(300ms 디바운스) + `tool=Bash` AND 조합.
  3. 필터 활성 중 그룹 카드 부재 + 서브에이전트 이벤트에 ⑂ 배지.
  4. URL에 `f_*` 파라미터 반영 → 새로고침해도 필터 유지.
  5. 필터 활성 중 위로 스크롤 → loadOlder가 필터 유지(네트워크 탭에서 파라미터 확인).
  6. 시그널 카드 점프(필터 비매칭 대상) → 필터 해제 알림 + 이동.
  7. (라이브 세션이 있으면) autoscroll OFF + 필터 활성 상태에서 "새 이벤트 ↓" 문구.

- [ ] **Step 3: 개선 루프 + 게이트** (CLAUDE.md PR-전 필수)

```bash
cd webui && node scripts/untagged-bash.ts --all
cd webui && node scripts/unknown-verification.ts --all
cd webui && node scripts/unidentified-plugins.ts --all
cd webui && node scripts/tagging-gate.ts
```

Expected: gate exit 0. 후보가 나오면 CLAUDE.md 개선 루프 절차대로 사전 추가 또는 baseline 보류 커밋.

- [ ] **Step 4: implementation-notes 기록** — 원장에 `#session-filtering-2026-07-04` 앵커 섹션 추가: (a) origin 분류 TS/Rust 의도적 이중화와 same-fixture 앵커 전략(§1.3), (b) flat 모드 채택·그룹 골격 유지 기각 사유, (c) 점프 규칙의 구현 형태(버퍼 내 존재 = 매칭으로 간주; 비존재 시 무조건 해제 — 서버 재질의 없이 결정론), (d) `list_session_window_kinds` 제거. notes-index.md의 "WebUI replay·목록" 행 최신 앵커를 갱신.

- [ ] **Step 5: 최종 검증 + 커밋 + PR**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -3
cd webui && npx vitest run 2>&1 | tail -3
git add docs/implementation-notes.html docs/notes-index.md
git commit -m "docs(notes): session filtering 구현 노트 — origin 이중화·flat 모드·점프 규칙"
git push -u origin feat/session-filtering
gh pr create --title "feat: 세션 이벤트 4축 서버 필터 + 리플레이 FilterBar (스펙 §1)" \
  --body "$(cat <<'EOF'
docs/specs/2026-07-04-session-detail-improvements.md §1 구현.
- API: /events 4축 필터(AND/CSV-OR)·matched_count·around×필터 400
- origin_of Rust 이식(message_origin_v01 동일 fixture 앵커, TS와 의도적 이중화)
- WebUI: FilterBar·URL 동기화·flat 모드·점프 시 해제·"새 이벤트 ↓"
- 개선 루프·브라우저 smoke 완료
EOF
)"
```

PR 병합은 사용자 몫(no-self-merge) — CI 통과 확인까지만.

---

## 스펙 §1 커버리지 맵 (셀프 리뷰용)

| 스펙 요구 | 태스크 |
|---|---|
| 8 파라미터 + CSV OR + 축 AND (§1.2 표) | T2, T5 |
| SQL 푸시다운 + Rust 술어 스캔 | T4, T5 |
| signal 서브쿼리/조인 | T3, T5 |
| matched_count (필터 시에만) | T4, T5 |
| around×필터 400 | T5 |
| origin_of 이식 + real fixture 앵커 + 이중화 편차 기록 | T1, T12 |
| FilterBar(칩·디바운스 ≥300ms·매칭 수) | T10 |
| URL 동기화 | T7, T11 |
| 윈도우 리셋·loadOlder/loadNewer 필터 유지 | T8 |
| SSE 백필 필터 + "새 이벤트 ↓" | T8, T11 |
| flat 모드 + ⑂ 출처 배지 | T9 |
| 점프 시 필터 해제 + UI 알림 | T10(notice), T11(`jumpNeedsFilterClear` 순수 함수 + 테스트) |
| j/k/e 필터 목록 기준 | T11(무변경 확인) |
| Rust/TS 테스트 + 브라우저 smoke | T1~T11 각 스텝, T12 |
