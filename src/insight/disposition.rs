//! Deterministic tool-execution disposition classification.
//!
//! A `tool_result`의 content가 **실행되지 않았거나 중단된** 호출의 하니스 마커로
//! 시작하는 경우를 분류한다. 이 마커들은 Claude Code 하니스가 기계적으로 생성하는
//! 고정 문구라 prefix 매칭만으로 결정론적이다 — 출력 본문 중간에 *인용*된 동일
//! 문구는 prefix가 아니므로 매칭되지 않는다(코퍼스에서 인용 오탐 4건 실측, 모두
//! mid-content였음).
//!
//! Real-data anchoring: 네 마커 모두
//! `tests/fixtures/transcripts/real/disposition_v01.jsonl`에 동결된 실 payload로
//! 잠긴다 (`tests/transcript_disposition.rs`의 invariant assertion).
//! 코퍼스 실측(78 메인 세션 + 696 서브에이전트, 136,724 레코드):
//! user_rejected 44 · backgrounded 74 · cancelled ~24 · policy_denied 4.
//!
//! Disposition은 pass/fail이 **아니다** — "명령이 실행되어 실패"(outcome Failed)와
//! "실행 자체가 거부/취소/백그라운드 전환"은 다른 축이다. tool_failure 지표와
//! verification Tier-4 추정 모두 disposition이 잡힌 result에는 적용하면 안 된다.

/// Tool 호출이 정상 실행되지 않은 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    /// 사용자가 permission 프롬프트에서 거부 — "The user doesn't want to proceed…".
    UserRejected,
    /// PreToolUse hook이 차단 — "Hook <event>:<tool> denied this tool".
    PolicyDenied,
    /// 병렬 tool call 취소 — "<tool_use_error>Cancelled: parallel tool call…".
    Cancelled,
    /// 백그라운드 실행으로 전환 — content는 실제 출력이 아니라 안내문.
    /// "Command running in background with ID: …".
    Backgrounded,
}

/// `tool_result` content를 disposition으로 분류한다. 해당 없으면 `None`.
///
/// 모든 매칭은 **content 시작(prefix)** 기준 — 하니스 마커는 항상 content 맨 앞에
/// 오고(동결 fixture 4건 모두), 인용된 마커는 mid-content라 매칭되지 않는다.
pub fn classify_disposition(content: &str) -> Option<Disposition> {
    if content.starts_with("The user doesn't want to proceed with this tool use") {
        return Some(Disposition::UserRejected);
    }
    // "Hook <event>:<tool> denied this tool" — 가변 부분(<event>:<tool>)이 있어
    // 첫 줄의 prefix+suffix로 잠근다.
    if content.starts_with("Hook ")
        && content
            .lines()
            .next()
            .is_some_and(|l| l.trim_end().ends_with("denied this tool"))
    {
        return Some(Disposition::PolicyDenied);
    }
    if content.starts_with("<tool_use_error>Cancelled:") {
        return Some(Disposition::Cancelled);
    }
    if content.starts_with("Command running in background with ID:") {
        return Some(Disposition::Backgrounded);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_user_rejected_prefix() {
        // 실 payload 형태 (disposition_v01.jsonl, session 0daafa6e)
        let c = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file).";
        assert_eq!(classify_disposition(c), Some(Disposition::UserRejected));
    }

    #[test]
    fn classifies_policy_denied_prefix() {
        // 실 payload 형태 (disposition_v01.jsonl, session 6a254a2a)
        let c = "Hook PreToolUse:Bash denied this tool";
        assert_eq!(classify_disposition(c), Some(Disposition::PolicyDenied));
    }

    #[test]
    fn classifies_cancelled_prefix() {
        // 실 payload 형태 (disposition_v01.jsonl, session ed82aee9)
        let c =
            "<tool_use_error>Cancelled: parallel tool call Bash(F=$(ls -S ...))</tool_use_error>";
        assert_eq!(classify_disposition(c), Some(Disposition::Cancelled));
    }

    #[test]
    fn classifies_backgrounded_prefix() {
        // 실 payload 형태 (disposition_v01.jsonl, session 5864d6c7)
        let c = "Command running in background with ID: b3upcoakz. Output is being written to: /private/tmp/x.output.";
        assert_eq!(classify_disposition(c), Some(Disposition::Backgrounded));
    }

    #[test]
    fn quoted_marker_mid_content_is_not_classified() {
        // 코퍼스에서 실측된 오탐 형태: 분석 출력 안에 마커가 인용됨(4건, 전부 mid-content).
        let c = "scan results:\nThe user doesn't want to proceed with this tool use — 44 hits";
        assert_eq!(classify_disposition(c), None);
        let c2 = "grep found: Command running in background with ID: abc";
        assert_eq!(classify_disposition(c2), None);
    }

    #[test]
    fn ordinary_output_and_plain_tool_use_error_are_none() {
        assert_eq!(classify_disposition("test result: ok. 1 passed"), None);
        // Cancelled 이외의 tool_use_error는 disposition이 아니라 실행 실패(outcome Failed).
        assert_eq!(
            classify_disposition(
                "<tool_use_error>File has been modified since read</tool_use_error>"
            ),
            None
        );
    }
}
