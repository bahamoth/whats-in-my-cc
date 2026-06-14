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
    /// Workflow 툴이 띄운 자식이면 그 run id(`subagents/workflows/<runId>/…`).
    /// 일반 Task/Agent 사이드카면 None.
    pub workflow_run_id: Option<String>,
}

/// 경로가 subagent 사이드카면 세션·agent·(워크플로우면 run) id를 돌려준다.
/// 그 외 레이아웃은 None — ingest는 건드리지 않는다. 두 레이아웃을 처리한다:
///   - 일반:     `…/<sessionId>/subagents/agent-*.meta.json`
///   - 워크플로우: `…/<sessionId>/subagents/workflows/<runId>/agent-*.meta.json`
pub fn sidecar_path_parts(path: &Path) -> Option<SidecarRef> {
    let name = path.file_name()?.to_str()?;
    let agent_id = name.strip_prefix("agent-")?.strip_suffix(".meta.json")?;
    if agent_id.is_empty() {
        return None;
    }
    let parent = path.parent()?;
    let parent_name = parent.file_name()?.to_str()?;
    // 일반 레이아웃: parent == "subagents"
    if parent_name == "subagents" {
        let session_id = parent.parent()?.file_name()?.to_str()?;
        if session_id.is_empty() {
            return None;
        }
        return Some(SidecarRef {
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            workflow_run_id: None,
        });
    }
    // 워크플로우 레이아웃: parent == <runId>, 그 위가 "workflows", 그 위가 "subagents".
    let workflows = parent.parent()?;
    if workflows.file_name()?.to_str()? != "workflows" {
        return None;
    }
    let subagents = workflows.parent()?;
    if subagents.file_name()?.to_str()? != "subagents" {
        return None;
    }
    let session_id = subagents.parent()?.file_name()?.to_str()?;
    if session_id.is_empty() {
        return None;
    }
    Some(SidecarRef {
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
        workflow_run_id: Some(parent_name.to_string()),
    })
}

/// 경로가 워크플로우 subagent 파일(`…/subagents/workflows/<runId>/agent-*.{jsonl,meta.json}`)
/// 이면 그 run id를 돌려준다. 그 외(일반 subagent, 메인 transcript)면 None.
/// transcript ingest가 이벤트에 `workflow_run_id`를 붙이는 데 쓴다.
pub fn workflow_run_id_from_path(path: &Path) -> Option<String> {
    let run = path.parent()?; // <runId>
    if run.parent()?.file_name()?.to_str()? != "workflows" {
        return None;
    }
    if run.parent()?.parent()?.file_name()?.to_str()? != "subagents" {
        return None;
    }
    Some(run.file_name()?.to_str()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn workflow_run_id_from_workflow_path() {
        let p = Path::new("/x/SESS/subagents/workflows/wf_abc/agent-a1.jsonl");
        assert_eq!(workflow_run_id_from_path(p).as_deref(), Some("wf_abc"));
        let meta = Path::new("/x/SESS/subagents/workflows/wf_abc/agent-a1.meta.json");
        assert_eq!(workflow_run_id_from_path(meta).as_deref(), Some("wf_abc"));
        let plain = Path::new("/x/SESS/subagents/agent-a1.jsonl");
        assert_eq!(workflow_run_id_from_path(plain), None);
    }

    #[test]
    fn sidecar_parts_plain_layout() {
        let sc = sidecar_path_parts(Path::new("/x/SESS/subagents/agent-a1.meta.json")).unwrap();
        assert_eq!(sc.session_id, "SESS");
        assert_eq!(sc.agent_id, "a1");
        assert_eq!(sc.workflow_run_id, None);
    }

    #[test]
    fn sidecar_parts_handles_workflows_layer() {
        let sc = sidecar_path_parts(Path::new(
            "/x/SESS/subagents/workflows/wf_abc/agent-a1.meta.json",
        ))
        .unwrap();
        assert_eq!(sc.session_id, "SESS");
        assert_eq!(sc.agent_id, "a1");
        assert_eq!(sc.workflow_run_id.as_deref(), Some("wf_abc"));
    }
}
