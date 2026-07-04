//! §1.3 (docs/specs/2026-07-04-session-detail-improvements.md) — 이벤트 필터.
//! `origin_of`는 webui/src/components/replay/stream/messageOrigin.ts의 결정론
//! 이식이다(마커 우선 → isMeta 후행). 두 구현은 같은 real fixture
//! (message_origin_v01.jsonl)로 앵커되어 드리프트가 테스트로 잡힌다.

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

/// messageOrigin.ts `TEAMMATE_MESSAGE`의 이식:
/// `/^\s*(?:Another Claude session sent a message:\s*)?<teammate-message[\s>]/`.
/// 이는 하나의 복합 패턴이다 — optional 접두문 뒤에 `<teammate-message`가
/// 이어지고 그 바로 뒤 문자가 공백 또는 `>`여야 한다. "Another Claude session
/// sent a message: hi"(마커 없음)나 "<teammate-messages>foo"(접두 뒤 문자가
/// `s`)는 매치하지 않는다 — 접두문·마커를 독립 OR로 취급하면 오탐이 난다.
fn is_teammate_message(text: &str) -> bool {
    let t = text.trim_start();
    let t = t
        .strip_prefix("Another Claude session sent a message:")
        .map(str::trim_start)
        .unwrap_or(t);
    match t.strip_prefix("<teammate-message") {
        Some(rest) => rest.starts_with(char::is_whitespace) || rest.starts_with('>'),
        None => false,
    }
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
        ],
    ) || is_teammate_message(text)
}

/// messageOrigin.ts `messageOrigin`의 이식. 마커 판정 순서는 TS와 동일하게
/// 유지한다(순서가 계약) — command → command-output → system → notification
/// → teammate → skill-scaffold → isMeta → human.
pub fn origin_of(payload: &Value, is_meta: bool) -> Origin {
    let text = user_message_text(payload);
    if starts_with_any(
        text,
        &["<command-name>", "<command-message>", "<command-args>"],
    ) {
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
    if is_teammate_message(text) {
        return Origin::Teammate;
    }
    if starts_with_any(text, &["Base directory for this skill:"]) {
        return Origin::Skill;
    }
    if is_meta {
        return Origin::Skill;
    }
    Origin::Human
}

use crate::model::observed::ObservedEvent;

/// 축 파싱의 원시 입력 — routes.rs `EventsQuery`의 필드를 그대로 받는다.
#[derive(Default)]
pub struct RawFilterParams<'a> {
    pub kind: Option<&'a str>,
    pub role: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub error: Option<&'a str>,
    pub signal: Option<&'a str>,
    pub verification: Option<&'a str>,
    pub tool: Option<&'a str>,
    pub model: Option<&'a str>,
    pub q: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => return None,
        })
    }

    /// role 축은 kind 매핑으로 판정한다(§1.2 — payload role 재파싱 불필요:
    /// ingest가 kind로 이미 분해). user→user_message, assistant→assistant_message,
    /// system→system_summary.
    fn kind_str(&self) -> &'static str {
        match self {
            Role::User => "user_message",
            Role::Assistant => "assistant_message",
            Role::System => "system_summary",
        }
    }
}

/// §1.2 8축 필터 — 축끼리 AND, 축 내부 CSV는 OR.
pub struct EventFilter {
    pub kinds: Option<Vec<String>>,
    pub roles: Option<Vec<Role>>,
    pub origins: Option<Vec<Origin>>,
    pub error: bool,
    pub signal: bool,
    pub verifications: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub models: Option<Vec<String>>,
    pub q: Option<String>,
}

/// `signal`/`verification` 축은 이벤트 자체에 없는 파생 정보가 필요하다 —
/// 호출부가 미리 조회해 넘기는 컨텍스트.
#[derive(Default)]
pub struct FilterCtx {
    pub signal_evidence: std::collections::HashSet<String>,
    pub verification_by_trigger: std::collections::HashMap<String, String>,
}

fn csv(s: &str) -> Vec<&str> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .collect()
}

fn parse_csv_enum<T>(
    s: Option<&str>,
    parse: fn(&str) -> Option<T>,
    axis: &str,
) -> Result<Option<Vec<T>>, String> {
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

impl EventFilter {
    pub fn from_params(p: &RawFilterParams) -> Result<Option<EventFilter>, String> {
        // kind: EventKind taxonomy 검증 (routes.rs의 기존 인라인 검증을 이관).
        let kinds = match p.kind {
            None => None,
            Some(s) => {
                let mut out = Vec::new();
                for k in csv(s) {
                    let ok = serde_json::from_value::<crate::model::observed::EventKind>(
                        serde_json::Value::String(k.to_string()),
                    )
                    .is_ok();
                    if !ok {
                        return Err(format!("unknown event kind: {k}"));
                    }
                    out.push(k.to_string());
                }
                if out.is_empty() {
                    None
                } else {
                    Some(out)
                }
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
                if vals.is_empty() {
                    None
                } else {
                    Some(vals)
                }
            }
        };
        let error = parse_bool_flag(p.error, "error")?;
        let signal = parse_bool_flag(p.signal, "signal")?;
        let tools = p
            .tool
            .map(|s| csv(s).iter().map(|x| x.to_string()).collect::<Vec<_>>())
            .filter(|v: &Vec<String>| !v.is_empty());
        let models = p
            .model
            .map(|s| csv(s).iter().map(|x| x.to_string()).collect::<Vec<_>>())
            .filter(|v: &Vec<String>| !v.is_empty());
        let q =
            p.q.map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase);
        let f = EventFilter {
            kinds,
            roles,
            origins,
            error,
            signal,
            verifications,
            tools,
            models,
            q,
        };
        if f.kinds.is_none()
            && f.roles.is_none()
            && f.origins.is_none()
            && !f.error
            && !f.signal
            && f.verifications.is_none()
            && f.tools.is_none()
            && f.models.is_none()
            && f.q.is_none()
        {
            return Ok(None);
        }
        Ok(Some(f))
    }

    pub fn needs_ctx(&self) -> bool {
        self.signal || self.verifications.is_some()
    }

    pub fn matches(&self, ev: &ObservedEvent, ctx: &FilterCtx) -> bool {
        if let Some(kinds) = &self.kinds {
            if !kinds.iter().any(|k| k == ev.kind.as_str()) {
                return false;
            }
        }
        if let Some(roles) = &self.roles {
            if !roles.iter().any(|r| r.kind_str() == ev.kind.as_str()) {
                return false;
            }
        }
        if let Some(origins) = &self.origins {
            // origin은 user_message에만 정의 — 다른 kind는 origin 축에서 탈락.
            if ev.kind != crate::model::observed::EventKind::UserMessage {
                return false;
            }
            let o = origin_of(&ev.payload, ev.is_meta);
            if !origins.contains(&o) {
                return false;
            }
        }
        if self.error {
            let is_err = ev
                .payload
                .pointer("/tool_result/is_error")
                .and_then(|v| v.as_bool())
                == Some(true);
            if !is_err {
                return false;
            }
        }
        if self.signal && !ctx.signal_evidence.contains(&ev.event_id) {
            return false;
        }
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
            if !searchable_text(ev).to_lowercase().contains(q.as_str()) {
                return false;
            }
        }
        true
    }
}

/// q 축의 검색 대상 문자열(§1.2): 메시지 본문 + 도구 입력/결과 문자열 필드.
fn searchable_text(ev: &ObservedEvent) -> String {
    let p = &ev.payload;
    let mut parts: Vec<String> = Vec::new();
    for key in ["content", "text", "thinking"] {
        if let Some(s) = p.get(key).and_then(|v| v.as_str()) {
            parts.push(s.to_string());
        }
    }
    if let Some(t) = &ev.tool_name {
        parts.push(t.clone());
    }
    if let Some(input) = p.get("input") {
        if !input.is_null() {
            parts.push(input.to_string());
        }
    }
    if let Some(tr) = p.get("tool_result") {
        match tr.get("content") {
            Some(Value::String(s)) => parts.push(s.clone()),
            Some(Value::Array(items)) => {
                for it in items {
                    if let Some(s) = it.get("text").and_then(|v| v.as_str()) {
                        parts.push(s.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_payload(s: &str) -> serde_json::Value {
        json!({ "content": s })
    }

    use crate::model::observed::{EventKind, ObservedEvent};

    fn ev(kind: EventKind, payload: serde_json::Value) -> ObservedEvent {
        ObservedEvent {
            kind,
            payload,
            ..Default::default()
        }
    }

    #[test]
    fn from_params_rejects_unknown_values() {
        let bad_origin = RawFilterParams {
            origin: Some("alien"),
            ..Default::default()
        };
        assert!(EventFilter::from_params(&bad_origin).is_err());
        let bad_role = RawFilterParams {
            role: Some("bot"),
            ..Default::default()
        };
        assert!(EventFilter::from_params(&bad_role).is_err());
        let bad_ver = RawFilterParams {
            verification: Some("flaky"),
            ..Default::default()
        };
        assert!(EventFilter::from_params(&bad_ver).is_err());
        let bad_kind = RawFilterParams {
            kind: Some("nope"),
            ..Default::default()
        };
        assert!(EventFilter::from_params(&bad_kind).is_err());
        let bad_error = RawFilterParams {
            error: Some("yes"),
            ..Default::default()
        };
        assert!(EventFilter::from_params(&bad_error).is_err()); // "true"만 허용
                                                                // 파라미터 전무 → None (필터 비활성)
        assert!(EventFilter::from_params(&RawFilterParams::default())
            .unwrap()
            .is_none());
    }

    #[test]
    fn matches_axes_and_combination() {
        let ctx = FilterCtx::default();
        // role=user → user_message kind 매핑
        let f = EventFilter::from_params(&RawFilterParams {
            role: Some("user"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert!(f.matches(
            &ev(EventKind::UserMessage, serde_json::json!({"content":"hi"})),
            &ctx
        ));
        assert!(!f.matches(
            &ev(
                EventKind::AssistantMessage,
                serde_json::json!({"text":"hi"})
            ),
            &ctx
        ));

        // error=true → tool_result.is_error
        let f = EventFilter::from_params(&RawFilterParams {
            error: Some("true"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert!(f.matches(
            &ev(
                EventKind::ToolResult,
                serde_json::json!({"tool_result":{"is_error":true,"content":"boom"}})
            ),
            &ctx
        ));
        assert!(!f.matches(
            &ev(
                EventKind::ToolResult,
                serde_json::json!({"tool_result":{"is_error":false,"content":"ok"}})
            ),
            &ctx
        ));

        // tool CSV OR (tool_name 컬럼)
        let f = EventFilter::from_params(&RawFilterParams {
            tool: Some("Bash,Edit"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        let mut e = ev(
            EventKind::ToolCall,
            serde_json::json!({"tool_name":"Bash","input":{}}),
        );
        e.tool_name = Some("Bash".into());
        assert!(f.matches(&e, &ctx));
        e.tool_name = Some("Read".into());
        assert!(!f.matches(&e, &ctx));

        // model 정확 일치
        let f = EventFilter::from_params(&RawFilterParams {
            model: Some("claude-fable-5"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert!(f.matches(
            &ev(
                EventKind::AssistantMessage,
                serde_json::json!({"text":"x","model":"claude-fable-5"})
            ),
            &ctx
        ));
        assert!(!f.matches(
            &ev(
                EventKind::AssistantMessage,
                serde_json::json!({"text":"x","model":"claude-haiku-4-5-20251001"})
            ),
            &ctx
        ));

        // q — 대소문자 무시, tool_result content 배열 텍스트 포함
        let f = EventFilter::from_params(&RawFilterParams {
            q: Some("PaNiC"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert!(f.matches(
            &ev(
                EventKind::ToolResult,
                serde_json::json!({"tool_result":{"content":[{"type":"text","text":"thread panicked"}]}})
            ),
            &ctx
        ));
        assert!(!f.matches(
            &ev(
                EventKind::UserMessage,
                serde_json::json!({"content":"calm"})
            ),
            &ctx
        ));

        // signal=true → ctx의 evidence set
        let f = EventFilter::from_params(&RawFilterParams {
            signal: Some("true"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert!(f.needs_ctx());
        let mut ctx2 = FilterCtx::default();
        ctx2.signal_evidence.insert("EV1".into());
        let mut e = ev(EventKind::ToolCall, serde_json::json!({}));
        e.event_id = "EV1".into();
        assert!(f.matches(&e, &ctx2));
        e.event_id = "EV2".into();
        assert!(!f.matches(&e, &ctx2));

        // verification=failed → ctx의 trigger 맵
        let f = EventFilter::from_params(&RawFilterParams {
            verification: Some("failed"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        let mut ctx3 = FilterCtx::default();
        ctx3.verification_by_trigger
            .insert("EV9".into(), "failed".into());
        let mut e = ev(EventKind::ToolCall, serde_json::json!({}));
        e.event_id = "EV9".into();
        assert!(f.matches(&e, &ctx3));

        // AND 조합: origin=human && q=deploy — 둘 다 만족해야 매칭
        let f = EventFilter::from_params(&RawFilterParams {
            origin: Some("human"),
            q: Some("deploy"),
            ..Default::default()
        })
        .unwrap()
        .unwrap();
        assert!(f.matches(
            &ev(
                EventKind::UserMessage,
                serde_json::json!({"content":"deploy it"})
            ),
            &ctx
        ));
        assert!(!f.matches(
            &ev(
                EventKind::UserMessage,
                serde_json::json!({"content":"ship it"})
            ),
            &ctx
        ));
        assert!(!f.matches(
            &ev(
                EventKind::UserMessage,
                serde_json::json!({"content":"<task-notification>deploy</task-notification>"})
            ),
            &ctx
        ));
    }

    #[test]
    fn origin_marker_cases_match_ts_classifier() {
        // messageOrigin.ts 87~103행과 1:1 — 순서·마커 정규식 계약.
        assert_eq!(
            origin_of(&text_payload("<command-name>/model</command-name>"), false),
            Origin::Command
        );
        assert_eq!(
            origin_of(
                &text_payload("<local-command-stdout>ok</local-command-stdout>"),
                true
            ),
            Origin::CommandOutput
        );
        assert_eq!(
            origin_of(&text_payload("[Request interrupted by user]"), false),
            Origin::System
        );
        assert_eq!(
            origin_of(
                &text_payload("<task-notification>\n<task-id>x</task-id>"),
                false
            ),
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
        assert_eq!(
            origin_of(&text_payload("fix the bug"), false),
            Origin::Human
        );
        // <system-reminder> 래핑은 human 유지 (messageOrigin.ts 99~103행 계약).
        assert_eq!(
            origin_of(
                &text_payload("<system-reminder>x</system-reminder>\nfix it"),
                false
            ),
            Origin::Human
        );
        // {"text": ...} 형태 payload(user_message content 배열 분해분)도 동일 판정.
        assert_eq!(
            origin_of(&json!({"text": "<command-name>/foo</command-name>"}), false),
            Origin::Command
        );
    }

    #[test]
    fn has_scaffold_marker_requires_compound_teammate_pattern() {
        // TS TEAMMATE_MESSAGE (messageOrigin.ts:50) is a single compound regex —
        // the optional "Another Claude session sent a message:" prefix must be
        // immediately followed by <teammate-message[\s>], not treated as two
        // independent alternatives. A naive prefix-OR over-matches both of
        // these counter-examples (regression for the bug found in review).
        assert!(!has_scaffold_marker(
            "Another Claude session sent a message: hi"
        ));
        assert!(!has_scaffold_marker("<teammate-messages>foo"));
        // Positive cases: marker present, char right after it is '>' or whitespace.
        assert!(has_scaffold_marker("<teammate-message>x"));
        assert!(has_scaffold_marker(
            "<teammate-message teammate_id=\"lead\">hi"
        ));
        assert!(has_scaffold_marker(
            "Another Claude session sent a message: <teammate-message teammate_id=\"lead\">hi"
        ));
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
            let Some(content) = rec.pointer("/message/content") else {
                continue;
            };
            let Some(text) = content.as_str() else {
                continue;
            };
            let is_meta = rec.get("isMeta").and_then(|m| m.as_bool()).unwrap_or(false);
            let o = origin_of(&json!({ "content": text }), is_meta);
            if text.trim_start().starts_with("<command-name>") {
                assert_eq!(
                    o,
                    Origin::Command,
                    "command marker must classify as command"
                );
                saw_command = true;
            } else if !is_meta && !super::has_scaffold_marker(text) {
                assert_eq!(
                    o,
                    Origin::Human,
                    "markerless non-meta must stay human: {text:.60}"
                );
                saw_human = true;
            }
        }
        assert!(
            saw_human && saw_command,
            "fixture must exercise both branches"
        );
    }
}
