//! piped(비-pager 파이프) 명령의 exit-파생 measured 신호 마스킹 검증.
//!
//! bash 매뉴얼: "The exit status of a pipeline is the exit status of the last
//! command in the pipeline" (Bash Reference Manual §3.7.5, pipefail 미설정 시).
//! 따라서 `cargo test … | grep … | head`의 shell-보고 exit는 `head`의 것이고,
//! 그 exit에서 파생된 모든 measured 신호 — OTLP `tool_result`의 `success`
//! attribute · hook post_tool_use `exit_code` · CC의 "Exit code N" content
//! prepend — 는 검증 도구의 결과가 아니다.
//!
//! 실사고 2026-07-06 (세션 bebd8197, vr_30c7c2a20327e4d6): transcript content가
//! "test result: FAILED. 2 passed; 1 failed"인데 OTLP success="true"(head exit
//! 0)를 measured로 신뢰해 passed/measured로 오판. 실 payload 3종을
//! tests/fixtures/observed/real/verification_piped_otlp_v01.json에 동결해 잠근다.
use chrono::{TimeZone, Utc};
use serde_json::json;
use wimcc::ingest::verification_run::extract_verification_runs;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

fn ts(i: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + i * 10, 0).unwrap()
}

fn base_ev(i: i64, kind: EventKind, tid: &str, payload: serde_json::Value) -> ObservedEvent {
    ObservedEvent {
        event_id: format!("ev_{i:03}"),
        raw_event_id: format!("raw_{i:03}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_piped".into(),
        observed_at: ts(i),
        actor: Actor::System,
        kind,
        tool_use_id: Some(tid.into()),
        tool_name: if kind == EventKind::ToolCall {
            Some("Bash".into())
        } else {
            None
        },
        parser_version: "test".into(),
        payload,
        ..Default::default()
    }
}

/// 동결한 실 payload 3종(tool_call · OTLP tool_result log · transcript
/// tool_result)으로 ObservedEvent를 재구성한다. invariant assertion으로 payload
/// 의미(파이프 명령·success="true"·FAILED 요약)를 먼저 잠근 뒤 추출한다.
fn real_piped_events() -> Vec<ObservedEvent> {
    let raw =
        std::fs::read_to_string("tests/fixtures/observed/real/verification_piped_otlp_v01.json")
            .expect("fixture");
    let f: serde_json::Value = serde_json::from_str(&raw).expect("fixture json");
    let tid = f["captured_from"]["tool_use_id"].as_str().expect("tid");
    let ev_of = |key: &str, i: i64, kind: EventKind| {
        base_ev(i, kind, tid, f["events"][key]["payload"].clone())
    };

    // ── invariant: 실 payload의 의미가 이 테스트의 전제와 일치하는지 잠금 ──
    let cmd = f["events"]["tool_call"]["payload"]["input"]["command"]
        .as_str()
        .expect("command");
    assert!(
        cmd.contains("| grep") && cmd.ends_with("| head"),
        "frozen command must be piped to non-pager grep then head: {cmd}"
    );
    assert_eq!(
        f["events"]["otlp_tool_result"]["payload"]["attributes"]["success"].as_str(),
        Some("true"),
        "frozen OTLP success must be \"true\" (pipeline exit 0 via head)"
    );
    let content = f["events"]["tool_result"]["payload"]["tool_result"]["content"]
        .as_str()
        .expect("content");
    assert!(
        content.contains("test result: FAILED"),
        "frozen transcript content must carry the cargo failure summary: {content}"
    );

    vec![
        ev_of("tool_call", 0, EventKind::ToolCall),
        ev_of("otlp_tool_result", 1, EventKind::LogRecord),
        ev_of("tool_result", 2, EventKind::ToolResult),
    ]
}

/// 실 payload: piped cargo test — OTLP success="true"(head exit 0)는 버려지고
/// content의 "test result: FAILED"(Tier-4)가 failed/estimated로 판정해야 한다.
#[test]
fn piped_otlp_success_is_discarded_failure_summary_wins() {
    let runs = extract_verification_runs(&real_piped_events());
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.status_basis, "piped");
    assert_eq!(
        run.status, "failed",
        "piped 명령의 OTLP success는 파이프 마지막 stage의 exit이므로 measured로 \
         신뢰하면 안 된다 — content 요약(FAILED)이 이긴다; got {:?}/{:?}",
        run.status, run.status_provenance
    );
    assert_eq!(run.status_provenance.as_deref(), Some("estimated"));
    assert!(run.failure_summary.is_some());
}

/// piped + hook post_tool_use exit_code=0 — 같은 이유로 버려진다.
#[test]
fn piped_hook_exit_code_is_discarded() {
    let tid = "tid_piped_hook";
    let evs = vec![
        base_ev(
            0,
            EventKind::ToolCall,
            tid,
            json!({"tool_use_id": tid, "name": "Bash",
                   "input": {"command": "cargo test 2>&1 | grep -E 'test result'"}}),
        ),
        {
            let mut e = base_ev(
                1,
                EventKind::HookEvent,
                tid,
                json!({"hook": {"tool_use_id": tid, "tool_response": {"exit_code": 0}}}),
            );
            e.subkind = Some("post_tool_use".into());
            e
        },
        base_ev(
            2,
            EventKind::ToolResult,
            tid,
            json!({"tool_result": {"tool_use_id": tid, "is_error": false,
                   "content": "test result: FAILED. 1 passed; 1 failed"}}),
        ),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].status, "failed",
        "hook exit 0도 파이프에 마스킹된다"
    );
    assert_eq!(runs[0].status_provenance.as_deref(), Some("estimated"));
}

/// piped + "Exit code 1" prepend만 있는 출력(예: grep 무매칭) — exit-파생이므로
/// 버려지고, content에 요약이 없으니 unknown으로 남아야 한다(추측 금지).
#[test]
fn piped_exit_code_prepend_without_summary_stays_unknown() {
    let tid = "tid_piped_prepend";
    let evs = vec![
        base_ev(
            0,
            EventKind::ToolCall,
            tid,
            json!({"tool_use_id": tid, "name": "Bash",
                   "input": {"command": "cargo test 2>&1 | grep -E 'nonexistent'"}}),
        ),
        base_ev(
            2,
            EventKind::ToolResult,
            tid,
            json!({"tool_result": {"tool_use_id": tid, "is_error": false,
                   "content": "Exit code 1"}}),
        ),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].status, "unknown",
        "파이프라인 exit(grep 무매칭 1)을 실패로 오판하면 안 된다"
    );
    assert_eq!(runs[0].status_basis, "piped");
}

/// 대조군: 비-pager 파이프가 없으면(pager tail은 exit basis) measured 신호는
/// 그대로 신뢰된다 — 기존 동작 무회귀.
#[test]
fn unpiped_otlp_success_stays_measured() {
    let tid = "tid_exit_otlp";
    let evs = vec![
        base_ev(
            0,
            EventKind::ToolCall,
            tid,
            json!({"tool_use_id": tid, "name": "Bash",
                   "input": {"command": "cargo test 2>&1 | tail -40"}}),
        ),
        base_ev(
            1,
            EventKind::LogRecord,
            tid,
            json!({"event_name": "tool_result", "attributes": {"tool_use_id": tid, "success": "true"}}),
        ),
        base_ev(
            2,
            EventKind::ToolResult,
            tid,
            json!({"tool_result": {"tool_use_id": tid, "is_error": false, "content": "ok"}}),
        ),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status_basis, "exit");
    assert_eq!(runs[0].status, "passed");
    assert_eq!(runs[0].status_provenance.as_deref(), Some("measured"));
}

/// piped여도 <tool_use_error>(하니스 채널, exit 무관)는 계속 Failed/measured.
#[test]
fn piped_tool_use_error_still_measured_failed() {
    let tid = "tid_piped_tue";
    let evs = vec![
        base_ev(
            0,
            EventKind::ToolCall,
            tid,
            json!({"tool_use_id": tid, "name": "Bash",
                   "input": {"command": "cargo test 2>&1 | grep x"}}),
        ),
        base_ev(
            2,
            EventKind::ToolResult,
            tid,
            json!({"tool_result": {"tool_use_id": tid, "is_error": true,
                   "content": "<tool_use_error>Command timed out</tool_use_error>"}}),
        ),
    ];
    let runs = extract_verification_runs(&evs);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "failed");
    assert_eq!(runs[0].status_provenance.as_deref(), Some("measured"));
}
