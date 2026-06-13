//! Subagent 사이드카 `meta.json` — 호출 관계의 원본.
//!
//! 실측(2026-06-13, CC 2.1.176, entrypoint remote_mobile, **표본 1** —
//! `tests/fixtures/transcripts/real/subagent_sidecar_v01/`에 동결): subagent
//! transcript는 메인 세션 jsonl이 아니라
//! `<sessionId>/subagents/agent-<agentId>.jsonl`로 떨어지고, 그 옆에
//! `agent-<agentId>.meta.json` = `{agentType, description, toolUseId}` 사이드카가
//! 남는다. `toolUseId`는 메인 체인 Task tool_use id와 일치 — jsonl 레코드에는
//! 이 연결고리가 없고(첫 sidechain 레코드 `parentUuid: null`) 오직 사이드카에만
//! 있다. meta.json 자체에는 sessionId·agentId가 없으므로 경로/파일명에서 끌어낸다.
//!
//! 로컬 CC가 같은 구조인지는 미확인 — 사이드카가 없으면 아무것도 만들지 않는
//! enrichment로만 동작한다(부재 시 degrade).

use std::path::Path;

pub const PARSER_VERSION: &str = "subagent_meta@v1";

/// 사이드카 경로에서 끌어낸 식별자. meta.json 내용에는 없는 값들이다.
#[derive(Debug, PartialEq)]
pub struct SidecarRef {
    /// `subagents/`의 부모 디렉터리명 = 세션 id (실측 레이아웃).
    pub session_id: String,
    /// 파일명 `agent-<agentId>.meta.json`의 가운데 토막.
    pub agent_id: String,
}

/// 경로가 subagent 사이드카(`…/<sessionId>/subagents/agent-*.meta.json`)이면
/// 세션·agent id를 돌려준다. 레이아웃이 다르면 None — ingest는 건드리지 않는다.
pub fn sidecar_path_parts(path: &Path) -> Option<SidecarRef> {
    let name = path.file_name()?.to_str()?;
    let agent_id = name.strip_prefix("agent-")?.strip_suffix(".meta.json")?;
    if agent_id.is_empty() {
        return None;
    }
    let parent = path.parent()?;
    if parent.file_name()?.to_str()? != "subagents" {
        return None;
    }
    let session_id = parent.parent()?.file_name()?.to_str()?;
    if session_id.is_empty() {
        return None;
    }
    Some(SidecarRef {
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
    })
}
