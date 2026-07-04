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
            let o = origin_of(&serde_json::json!({ "content": text }), is_meta);
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
