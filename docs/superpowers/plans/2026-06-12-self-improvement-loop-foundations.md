# Self-Improvement Loop Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 자기개선 루프가 닫히는 데 필요한 결정론 측정 기반 — 세션 환경 fingerprint(개입 귀속의 독립변수), 세션 횡단 metrics series(전후 비교), retrospect 스킬의 가설 원장 — 를 구축한다.

**Architecture:** 전부 기존 원칙 정합: (a) SessionStart hook 수신 시각에 서버가 CLAUDE.md 후보를 해시(관측이지 설정 변경 아님, 내용 미저장), (b) fingerprint·series는 SessionMetrics와 같은 on-demand 무저장 집계(count only, 판단 필드 없음), (c) 루프의 기억은 wimcc가 아니라 repo(dogfood/*.md + git)에 — 스킬이 오케스트레이션. migration 불필요(전부 payload/on-demand).

**Tech Stack:** Rust(axum, sqlx, sha2, dirs), 기존 MCP Streamable HTTP, 마크다운 스킬, 자기완결 HTML 사양서.

**배경(리뷰 발견과의 매핑):** 발견 1=Task 1–4, 발견 2=Task 5–6, 발견 6=Task 7, 발견 3·4=Task 8, 발견 7·우선순위 0=Task 9.

**Real-data anchoring:**
- SessionStart hook stdin의 공통 필드(session_id, cwd, hook_event_name, source)는 공식 docs(https://code.claude.com/docs/en/hooks)와 동결 fixture `tests/fixtures/hook/session_start.json`(cwd 없는 최소형 — degrade 케이스)으로 잠근다.
- CLAUDE.md 위치(조상 디렉터리 + `~/.claude/CLAUDE.md`)는 memory docs(https://code.claude.com/docs/en/memory) + 실 세션 관측(2026-06-12, 표본 1: `/Users/bahamoth/CLAUDE.md`(조상) + 프로젝트 CLAUDE.md가 주입됨)으로 뒷받침. **CC의 정확한 병합 의미론을 단정하지 않는다** — "시작 시점에 존재한 instruction 파일 후보의 관측"으로만 명세.
- transcript에는 CLAUDE.md가 기록되지 않음(2026-06-12 실측, 12 transcript/4 프로젝트 음성) — 과거 세션 소급 불가가 이 설계의 알려진 한계.
- `message.model`은 assistant payload에 실존(`tests/fixtures/transcripts/real/verification_v01.jsonl`, tests/payload_model.rs로 기잠금).

---

### Task 1: instruction snapshot 모듈

**Files:**
- Create: `src/ingest/instruction_snapshot.rs`
- Modify: `src/ingest/mod.rs` (모듈 등록 1줄)

- [ ] **Step 1: 모듈 뼈대 + 실패하는 단위 테스트 작성**

`src/ingest/instruction_snapshot.rs` 생성 — 테스트가 먼저 컴파일되도록 시그니처만 두고 본문은 `todo!()`:

```rust
//! SessionStart 수신 시각의 instruction 파일(CLAUDE.md) 스냅샷.
//!
//! 자기개선 루프의 독립변수(세션이 어떤 instruction 아래에서 돌았는가) 관측.
//! transcript에는 CLAUDE.md가 기록되지 않으므로(2026-06-12 실측: 12 transcript
//! 음성) hook 수신 "그 시점"에 서버가 후보 파일을 해시한다 — 관측이지 설정
//! 변경이 아니다(non-goal 비침해). 내용은 저장하지 않는다(경로+sha256+크기만);
//! 변경 감지는 해시 비교, 내용 복원은 git의 몫.
//!
//! 후보 = cwd→파일시스템 루트 조상 디렉터리의 `CLAUDE.md` + `home/.claude/CLAUDE.md`.
//! CC의 정확한 병합 의미론을 주장하지 않는다 — memory docs
//! (https://code.claude.com/docs/en/memory)의 위치 규칙 + 실 세션 관측(표본 1)
//! 기반의 "존재 후보 관측"이다. 읽기 실패는 조용히 건너뛴다(OBS-3 degrade).

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

/// 관측된 instruction 파일 하나. payload JSON(`/captured/claude_md[]`)에 그대로 직렬화.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstructionFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// cwd의 조상 경로 전체 + home/.claude 에서 존재하는 CLAUDE.md를 해시한다.
pub fn snapshot(cwd: &Path, home: Option<&Path>) -> Vec<InstructionFile> {
    todo!()
}
```

같은 파일 하단에 단위 테스트(프로젝트 관례: 모듈 내 `#[cfg(test)]`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// std::env::temp_dir() 아래 고유 디렉터리 (tempfile dev-dep 불필요).
    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir()
            .join("wimcc_snap_test")
            .join(format!("{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha_hex(s: &str) -> String {
        hex::encode(Sha256::digest(s.as_bytes()))
    }

    #[test]
    fn collects_ancestor_and_home_claude_md() {
        let root = scratch("collect");
        let cwd = root.join("a").join("b");
        fs::create_dir_all(&cwd).unwrap();
        fs::write(root.join("CLAUDE.md"), "root rules").unwrap();
        fs::write(cwd.join("CLAUDE.md"), "project rules").unwrap();
        let home = root.join("home");
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(home.join(".claude").join("CLAUDE.md"), "user rules").unwrap();

        let got = snapshot(&cwd, Some(&home));

        // 조상 walk가 scratch 밖(/tmp, /)으로도 올라가므로 정확 길이 대신 포함을 단언.
        let find = |p: &std::path::Path| {
            got.iter()
                .find(|f| f.path == p.to_string_lossy())
                .unwrap_or_else(|| panic!("missing {}", p.display()))
                .clone()
        };
        let proj = find(&cwd.join("CLAUDE.md"));
        assert_eq!(proj.sha256, sha_hex("project rules"));
        assert_eq!(proj.bytes, "project rules".len() as u64);
        let root_f = find(&root.join("CLAUDE.md"));
        assert_eq!(root_f.sha256, sha_hex("root rules"));
        let user = find(&home.join(".claude").join("CLAUDE.md"));
        assert_eq!(user.sha256, sha_hex("user rules"));
        // 중복 경로 없음.
        let mut paths: Vec<&str> = got.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), got.len(), "duplicate paths in snapshot");
    }

    #[test]
    fn missing_files_are_skipped_silently() {
        let root = scratch("missing");
        let cwd = root.join("x");
        fs::create_dir_all(&cwd).unwrap();
        let got = snapshot(&cwd, None);
        assert!(
            got.iter().all(|f| !f.path.starts_with(root.to_string_lossy().as_ref())),
            "no CLAUDE.md under scratch must yield no scratch entries"
        );
    }
}
```

`src/ingest/mod.rs`에 `pub mod instruction_snapshot;` 추가.

- [ ] **Step 2: 테스트가 빨강인지 확인**

Run: `cargo test --lib instruction_snapshot 2>&1 | tail -5`
Expected: FAIL (panic: `not yet implemented` — todo!())

- [ ] **Step 3: 최소 구현**

```rust
pub fn snapshot(cwd: &Path, home: Option<&Path>) -> Vec<InstructionFile> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        candidates.push(d.join("CLAUDE.md"));
        dir = d.parent();
    }
    if let Some(h) = home {
        candidates.push(h.join(".claude").join("CLAUDE.md"));
    }
    let mut out = Vec::new();
    for p in candidates {
        let Ok(content) = std::fs::read(&p) else {
            continue;
        };
        out.push(InstructionFile {
            path: p.to_string_lossy().into_owned(),
            sha256: hex::encode(Sha256::digest(&content)),
            bytes: content.len() as u64,
        });
    }
    out
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test --lib instruction_snapshot 2>&1 | tail -5`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add src/ingest/instruction_snapshot.rs src/ingest/mod.rs
git commit -m "feat(ingest): instruction snapshot 모듈 (CLAUDE.md 후보 sha256 관측)"
```

---

### Task 2: hook store가 session_start 수신 시 스냅샷을 capture

**Files:**
- Modify: `src/ingest/hook.rs` (store에 capture 분기 + `store_with_env`)
- Test: `tests/hook_session_start_snapshot.rs`

- [ ] **Step 1: 실패하는 통합 테스트 작성**

`tests/hook_session_start_snapshot.rs`:

```rust
//! SessionStart hook 수신 시각의 CLAUDE.md 스냅샷 capture.
//!
//! Real-data anchoring: cwd 없는 동결 fixture(tests/fixtures/hook/session_start.json)
//! 는 capture가 발생하지 않는 degrade 케이스를 잠근다. cwd 있는 케이스는
//! 공식 docs(https://code.claude.com/docs/en/hooks)의 공통 stdin 필드(cwd)를 따른다.

use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed};
use wimcc::ingest::hook::{parse_body, store_with_env, SnapshotEnv};
use wimcc::live::NoopSink;

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir()
        .join("wimcc_hook_snap")
        .join(format!("{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test]
async fn session_start_with_cwd_captures_claude_md_hashes() {
    let pool = test_pool().await;
    let root = scratch("capture");
    let cwd = root.join("proj");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "rules v1").unwrap();
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude").join("CLAUDE.md"), "user rules").unwrap();

    let body = json!({
        "session_id": "sess_snap_1",
        "hook_event_name": "SessionStart",
        "source": "startup",
        "cwd": cwd.to_string_lossy(),
    });
    let env = SnapshotEnv { home: Some(home.clone()) };
    store_with_env(&pool, parse_body(&body), chrono::Utc::now(), &NoopSink, &env)
        .await
        .unwrap();

    let rows = repo_observed::list_session(&pool, "sess_snap_1", 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let captured = rows[0]
        .payload
        .pointer("/captured/claude_md")
        .expect("session_start payload must carry /captured/claude_md")
        .as_array()
        .unwrap()
        .clone();
    let proj_path = cwd.join("CLAUDE.md").to_string_lossy().into_owned();
    let user_path = home.join(".claude").join("CLAUDE.md").to_string_lossy().into_owned();
    let by_path = |p: &str| -> Value {
        captured
            .iter()
            .find(|e| e["path"].as_str() == Some(p))
            .unwrap_or_else(|| panic!("missing snapshot entry {p}"))
            .clone()
    };
    use sha2::{Digest, Sha256};
    assert_eq!(
        by_path(&proj_path)["sha256"].as_str().unwrap(),
        hex::encode(Sha256::digest(b"rules v1"))
    );
    assert_eq!(by_path(&user_path)["bytes"].as_u64().unwrap(), "user rules".len() as u64);
    assert!(rows[0].payload.pointer("/captured/captured_at").is_some());
    // raw(hook 원문)는 그대로 보존 — capture는 observed payload에만 붙는다.
    assert!(rows[0].payload.pointer("/hook/session_id").is_some());
}

#[tokio::test]
async fn session_start_without_cwd_degrades_to_no_capture() {
    let pool = test_pool().await;
    // 동결 실 fixture: cwd 없는 최소형.
    let body: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/hook/session_start.json").unwrap(),
    )
    .unwrap();
    let env = SnapshotEnv { home: None };
    store_with_env(&pool, parse_body(&body), chrono::Utc::now(), &NoopSink, &env)
        .await
        .unwrap();
    let rows = repo_observed::list_session(&pool, "sess_fix_A", 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].payload.pointer("/captured").is_none(),
        "no cwd → no capture key at all"
    );
}

#[tokio::test]
async fn non_session_start_hooks_never_capture() {
    let pool = test_pool().await;
    let body: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/hook/pre_tool_use.json").unwrap(),
    )
    .unwrap();
    let env = SnapshotEnv { home: None };
    store_with_env(&pool, parse_body(&body), chrono::Utc::now(), &NoopSink, &env)
        .await
        .unwrap();
    let rows = repo_observed::list_session(&pool, "sess_fix_A", 10).await.unwrap();
    assert!(rows.iter().all(|r| r.payload.pointer("/captured").is_none()));
}
```

주의: `pre_tool_use.json` fixture의 session_id가 `sess_fix_A`가 아니면 세 번째 테스트의 session_id를 fixture 값으로 맞춘다(실행 시 확인).

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --test hook_session_start_snapshot 2>&1 | tail -5`
Expected: COMPILE FAIL (`store_with_env`/`SnapshotEnv` 없음)

- [ ] **Step 3: 구현 — store_with_env + capture 분기**

`src/ingest/hook.rs`:

```rust
/// 스냅샷 환경 — 테스트에서 home을 주입하기 위한 분리(실 경로는 dirs::home_dir()).
#[derive(Debug, Default)]
pub struct SnapshotEnv {
    pub home: Option<std::path::PathBuf>,
}

pub async fn store(
    pool: &SqlitePool,
    parsed: ParseResult,
    received_at: DateTime<Utc>,
    sink: &dyn LiveSink,
) -> Result<IngestResult> {
    let env = SnapshotEnv { home: dirs::home_dir() };
    store_with_env(pool, parsed, received_at, sink, &env).await
}

pub async fn store_with_env(
    pool: &SqlitePool,
    parsed: ParseResult,
    received_at: DateTime<Utc>,
    sink: &dyn LiveSink,
    env: &SnapshotEnv,
) -> Result<IngestResult> {
    // (기존 store 본문 이동)
}
```

기존 본문에서 ObservedEvent payload 구성부를 다음으로 교체:

```rust
        let mut payload = serde_json::json!({"hook": ev.raw});
        // SessionStart 수신 "그 시점"의 instruction 스냅샷 — 자기개선 루프의
        // 독립변수 관측. cwd 없으면 관측 불가로 degrade(키 자체를 만들지 않음).
        if ev.subkind == "session_start" {
            if let Some(cwd) = ev.cwd.as_deref() {
                let files = crate::ingest::instruction_snapshot::snapshot(
                    std::path::Path::new(cwd),
                    env.home.as_deref(),
                );
                if !files.is_empty() {
                    payload["captured"] = serde_json::json!({
                        "claude_md": files,
                        "captured_at": received_at.to_rfc3339(),
                    });
                }
            }
        }
        let event = ObservedEvent {
            // … 기존 필드 그대로, payload만 위 변수 사용
            payload,
            // …
        };
```

- [ ] **Step 4: 통과 확인 (기존 hook 테스트 포함)**

Run: `cargo test --test hook_session_start_snapshot && cargo test --lib ingest::hook 2>&1 | tail -3`
Expected: PASS (신규 3 + 기존 전부)

- [ ] **Step 5: Commit**

```bash
git add src/ingest/hook.rs tests/hook_session_start_snapshot.rs
git commit -m "feat(ingest): SessionStart 수신 시각 CLAUDE.md 스냅샷 capture"
```

---

### Task 3: session fingerprint 모듈 (on-demand)

**Files:**
- Create: `src/insight/fingerprint.rs`
- Modify: `src/insight/mod.rs` (모듈 등록)
- Test: `tests/fingerprint_compute.rs`

- [ ] **Step 1: 실패하는 테스트 작성**

`tests/fingerprint_compute.rs`:

```rust
//! 세션 환경 fingerprint — 개입(구성) 귀속의 독립변수.
//! SessionMetrics와 같은 on-demand 무저장 패턴. 모든 Vec은 정렬·distinct.

use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::insight::fingerprint::compute_session_fingerprint;
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    sid: &str,
    eid: &str,
    kind: EventKind,
    subkind: Option<&str>,
    payload: serde_json::Value,
    cc_version: Option<&str>,
    git_branch: Option<&str>,
) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/fp.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{eid}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: eid.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: sid.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind,
        subkind: subkind.map(String::from),
        payload,
        cwd: Some("/proj/x".into()),
        entrypoint: Some("cli".into()),
        cc_version: cc_version.map(String::from),
        git_branch: git_branch.map(String::from),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn fingerprint_collects_distinct_sorted_env_and_models() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_fp_1";
    seed(&pool, &run, sid, "a1", EventKind::AssistantMessage, None,
         json!({"model": "claude-opus-4-7"}), Some("2.1.0"), Some("main")).await;
    seed(&pool, &run, sid, "a2", EventKind::AssistantMessage, None,
         json!({"model": "claude-opus-4-7"}), Some("2.1.0"), Some("main")).await;
    seed(&pool, &run, sid, "a3", EventKind::AssistantMessage, None,
         json!({"model": "claude-haiku-4-5"}), Some("2.1.1"), Some("feat/x")).await;
    // 모델 없는 user 이벤트는 무시되어야 함
    seed(&pool, &run, sid, "u1", EventKind::UserMessage, None,
         json!({"text": "hi"}), Some("2.1.0"), Some("main")).await;

    let fp = compute_session_fingerprint(&pool, sid).await.unwrap();
    assert_eq!(fp.session_id, sid);
    assert_eq!(fp.models, vec!["claude-haiku-4-5", "claude-opus-4-7"]);
    assert_eq!(fp.cc_versions, vec!["2.1.0", "2.1.1"]);
    assert_eq!(fp.git_branches, vec!["feat/x", "main"]);
    assert_eq!(fp.cwds, vec!["/proj/x"]);
    assert_eq!(fp.entrypoints, vec!["cli"]);
    assert!(fp.claude_md.is_empty());
    assert!(fp.instruction_sha256.is_none());
}

#[tokio::test]
async fn fingerprint_reads_session_start_snapshot_union() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    let sid = "sess_fp_2";
    let snap = |sha: &str| {
        json!({
            "hook": {"hook_event_name": "SessionStart"},
            "captured": {
                "claude_md": [
                    {"path": "/p/CLAUDE.md", "sha256": sha, "bytes": 10}
                ],
                "captured_at": "2026-06-12T00:00:00Z"
            }
        })
    };
    seed(&pool, &run, sid, "h1", EventKind::HookEvent, Some("session_start"),
         snap("aaaa"), None, None).await;
    // 같은 (path,sha) 재수신(세션 재개) — union dedup
    seed(&pool, &run, sid, "h2", EventKind::HookEvent, Some("session_start"),
         snap("aaaa"), None, None).await;

    let fp = compute_session_fingerprint(&pool, sid).await.unwrap();
    assert_eq!(fp.claude_md.len(), 1);
    assert_eq!(fp.claude_md[0].path, "/p/CLAUDE.md");
    assert_eq!(fp.claude_md[0].sha256, "aaaa");
    let expected = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(b"/p/CLAUDE.md\naaaa\n"))
    };
    assert_eq!(fp.instruction_sha256.as_deref(), Some(expected.as_str()));
}

#[tokio::test]
async fn fingerprint_empty_session_is_all_empty() {
    let pool = test_pool().await;
    let fp = compute_session_fingerprint(&pool, "sess_fp_none").await.unwrap();
    assert!(fp.models.is_empty());
    assert!(fp.instruction_sha256.is_none());
}
```

- [ ] **Step 2: 빨강 확인**

Run: `cargo test --test fingerprint_compute 2>&1 | tail -5`
Expected: COMPILE FAIL (`insight::fingerprint` 없음)

- [ ] **Step 3: 구현**

`src/insight/fingerprint.rs`:

```rust
//! 세션 환경 fingerprint — "이 세션은 어떤 구성 아래에서 돌았는가"의 결정론 관측.
//!
//! 자기개선 루프의 독립변수: 개입(CLAUDE.md/스킬/모델 변경)의 효과를 세션
//! 코호트로 귀속하려면 구성의 관측이 선행해야 한다. SessionMetrics와 같은
//! on-demand 무저장 패턴(§10.1) — count/관측 값만, 판단 필드 없음(§6.3).
//!
//! 출처: models = assistant payload `/model`(재ingest 필요한 payload 필드),
//! cc_versions/git_branches/cwds/entrypoints = observed_event 컬럼,
//! claude_md = session_start hook의 `/captured/claude_md`(Task 2).
//! transcript에 CLAUDE.md가 없으므로(2026-06-12 실측) hook 미설치 세션은
//! claude_md가 비고 instruction_sha256은 None — 결측은 결측으로 노출한다.

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::error::Result;
use crate::ingest::instruction_snapshot::InstructionFile;

#[derive(Debug, Clone, Serialize)]
pub struct SessionFingerprint {
    pub session_id: String,
    /// assistant_message payload `/model`의 distinct 정렬 목록.
    pub models: Vec<String>,
    pub cc_versions: Vec<String>,
    pub git_branches: Vec<String>,
    pub cwds: Vec<String>,
    pub entrypoints: Vec<String>,
    /// session_start 스냅샷의 (path,sha256,bytes) union — path 정렬.
    pub claude_md: Vec<InstructionFile>,
    /// claude_md의 정렬 (path,sha) 결합 sha256 — 코호트 group key.
    /// 스냅샷이 없으면 None(결측 ≠ 빈 instruction).
    pub instruction_sha256: Option<String>,
}

async fn distinct_column(pool: &SqlitePool, session_id: &str, col: &str) -> Result<Vec<String>> {
    // col은 아래 호출부의 고정 문자열만 — 사용자 입력이 아니다.
    let sql = format!(
        "SELECT DISTINCT {col} AS v FROM observed_event \
         WHERE session_id = ? AND {col} IS NOT NULL ORDER BY v"
    );
    let rows = sqlx::query(&sql).bind(session_id).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("v")).collect())
}

pub async fn compute_session_fingerprint(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<SessionFingerprint> {
    let models_rows = sqlx::query(
        "SELECT DISTINCT json_extract(payload,'$.model') AS m FROM observed_event \
         WHERE session_id = ? AND kind = 'assistant_message' \
           AND json_extract(payload,'$.model') IS NOT NULL ORDER BY m",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let models = models_rows
        .iter()
        .map(|r| r.get::<String, _>("m"))
        .collect();

    let snap_rows = sqlx::query(
        "SELECT payload FROM observed_event \
         WHERE session_id = ? AND kind = 'hook_event' AND subkind = 'session_start'",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut claude_md: Vec<InstructionFile> = Vec::new();
    for row in &snap_rows {
        let payload: serde_json::Value =
            serde_json::from_str(&row.get::<String, _>("payload")).unwrap_or_default();
        let Some(files) = payload.pointer("/captured/claude_md").and_then(|v| v.as_array())
        else {
            continue;
        };
        for f in files {
            let (Some(path), Some(sha)) = (f["path"].as_str(), f["sha256"].as_str()) else {
                continue;
            };
            let entry = InstructionFile {
                path: path.to_string(),
                sha256: sha.to_string(),
                bytes: f["bytes"].as_u64().unwrap_or(0),
            };
            if !claude_md.iter().any(|e| e.path == entry.path && e.sha256 == entry.sha256) {
                claude_md.push(entry);
            }
        }
    }
    claude_md.sort_by(|a, b| a.path.cmp(&b.path).then(a.sha256.cmp(&b.sha256)));
    let instruction_sha256 = if claude_md.is_empty() {
        None
    } else {
        let mut h = Sha256::new();
        for f in &claude_md {
            h.update(f.path.as_bytes());
            h.update(b"\n");
            h.update(f.sha256.as_bytes());
            h.update(b"\n");
        }
        Some(hex::encode(h.finalize()))
    };

    Ok(SessionFingerprint {
        session_id: session_id.to_string(),
        models,
        cc_versions: distinct_column(pool, session_id, "cc_version").await?,
        git_branches: distinct_column(pool, session_id, "git_branch").await?,
        cwds: distinct_column(pool, session_id, "cwd").await?,
        entrypoints: distinct_column(pool, session_id, "entrypoint").await?,
        claude_md,
        instruction_sha256,
    })
}
```

`src/insight/mod.rs`에 `pub mod fingerprint;` 추가.

주의(실행 시 확인): ① `repo_observed::insert`가 payload를 어떻게 직렬화하는지에 따라 `SELECT payload` 후 `from_str` 부분을 맞춘다(컬럼 TEXT). ② `EventKind` DB 표현은 `as_str()` snake_case("assistant_message", "hook_event") — `src/model/observed.rs:51` 확인됨. ③ ObservedEvent에 `subkind`·`cc_version` 등 setter 필드가 실제로 있는지는 mapping.rs 기준 확인됨.

- [ ] **Step 4: 통과 확인**

Run: `cargo test --test fingerprint_compute 2>&1 | tail -5`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add src/insight/fingerprint.rs src/insight/mod.rs tests/fingerprint_compute.rs
git commit -m "feat(insight): 세션 환경 fingerprint on-demand 집계"
```

---

### Task 4: GET /v1/sessions/:id/fingerprint

**Files:**
- Modify: `src/api/routes.rs` (핸들러), `src/api/mod.rs` (라우트 1줄)
- Test: `tests/api_fingerprint.rs`

- [ ] **Step 1: 실패하는 테스트**

`tests/api_fingerprint.rs` (api_metrics.rs의 seed/build_server 패턴 복사):

```rust
//! GET /v1/sessions/:id/fingerprint — envelope + 결정론 관측 필드.

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn seed_assistant(pool: &sqlx::SqlitePool, run_id: &str, sid: &str, eid: &str, model: &str) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/fpapi.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{eid}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: eid.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: sid.into(),
        observed_at: chrono::Utc::now(),
        actor: Actor::Assistant,
        kind: EventKind::AssistantMessage,
        payload: json!({"model": model}),
        cc_version: Some("2.1.0".into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

#[tokio::test]
async fn fingerprint_endpoint_returns_envelope_with_models() {
    let pool = test_pool().await;
    let run = repo_runs::start(&pool).await.unwrap();
    seed_assistant(&pool, &run, "sess_fp_api", "e1", "claude-opus-4-7").await;
    let server = TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap();
    let r = server.get("/v1/sessions/sess_fp_api/fingerprint").await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body["meta"]["schema_version"].is_string());
    let data = &body["data"];
    assert_eq!(data["session_id"], "sess_fp_api");
    assert_eq!(data["models"][0], "claude-opus-4-7");
    assert_eq!(data["cc_versions"][0], "2.1.0");
    assert!(data["claude_md"].as_array().unwrap().is_empty());
    assert!(data["instruction_sha256"].is_null());
}
```

- [ ] **Step 2: 빨강 확인** — Run: `cargo test --test api_fingerprint 2>&1 | tail -3` / Expected: 404 assert 실패

- [ ] **Step 3: 구현**

`src/api/routes.rs` (session_metrics 아래):

```rust
/// `GET /v1/sessions/:id/fingerprint` — 세션 환경 fingerprint (on-demand).
/// 자기개선 루프의 독립변수 표면: 어떤 모델·CC버전·branch·instruction 아래에서
/// 돌았는가. 관측 값만 — 판단 필드 없음.
pub async fn session_fingerprint(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(gone) = tombstone_gate(&pool, &id, "session", "session").await {
        return gone.into_response();
    }
    match crate::insight::fingerprint::compute_session_fingerprint(&pool, &id).await {
        Ok(f) => Json(Envelope { meta: ResponseMeta::now(), data: f }).into_response(),
        Err(err) => {
            tracing::error!(session_id = %id, err = %err, "session_fingerprint failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}
```

`src/api/mod.rs` 라우터에:

```rust
        .route("/v1/sessions/:id/fingerprint", get(routes::session_fingerprint))
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --test api_fingerprint` / Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/api/routes.rs src/api/mod.rs tests/api_fingerprint.rs
git commit -m "feat(api): GET /v1/sessions/:id/fingerprint"
```

---

### Task 5: 세션 횡단 series — src/insight/series.rs + GET /v1/metrics

**Files:**
- Create: `src/insight/series.rs` (HTTP·MCP 공용 수집기)
- Modify: `src/insight/mod.rs`, `src/api/routes.rs`, `src/api/mod.rs`
- Test: `tests/api_metrics_sessions.rs`

- [ ] **Step 1: 실패하는 테스트**

`tests/api_metrics_sessions.rs` — 핵심 단언:

```rust
//! GET /v1/metrics — 프로젝트/기간 필터의 세션 횡단 metrics+fingerprint series.
//! 전후 비교(개입 효과 귀속)의 측정면. count only — rate·판단은 소비자 몫.

use axum_test::TestServer;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use wimcc::api::AppState;
use wimcc::db::{migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    pool
}

async fn seed_tool_call(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    sid: &str,
    eid: &str,
    cwd: &str,
    observed_at: chrono::DateTime<chrono::Utc>,
) {
    let raw_id = format!("raw_{eid}");
    repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.into(),
            source_type: "claude_transcript".into(),
            source_uri: "/tmp/ms.jsonl".into(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: format!("sha_{eid}"),
            payload: b"{}".to_vec(),
            parse_error: None,
            captured_at: chrono::Utc::now(),
            redaction_state: "not_applicable".into(),
            redaction_manifest: None,
        },
    )
    .await
    .unwrap();
    let e = ObservedEvent {
        event_id: eid.into(),
        raw_event_id: raw_id,
        schema_version: "observed_event.v1".into(),
        session_id: sid.into(),
        observed_at,
        actor: Actor::Assistant,
        kind: EventKind::ToolCall,
        cwd: Some(cwd.into()),
        parser_version: "test@v0".into(),
        ..Default::default()
    };
    repo_observed::insert(pool, &e).await.unwrap();
}

fn t(rfc3339: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

async fn seed_three_sessions(pool: &sqlx::SqlitePool) {
    let run = repo_runs::start(pool).await.unwrap();
    // 프로젝트 A: 6/1과 6/10 두 세션, 프로젝트 B: 6/5 한 세션
    seed_tool_call(pool, &run, "sess_a_old", "ms1", "/proj/A", t("2026-06-01T00:00:00Z")).await;
    seed_tool_call(pool, &run, "sess_a_new", "ms2", "/proj/A", t("2026-06-10T00:00:00Z")).await;
    seed_tool_call(pool, &run, "sess_a_new", "ms3", "/proj/A", t("2026-06-10T01:00:00Z")).await;
    seed_tool_call(pool, &run, "sess_b", "ms4", "/proj/B", t("2026-06-05T00:00:00Z")).await;
}

fn server(pool: sqlx::SqlitePool) -> TestServer {
    TestServer::new(wimcc::api::router(AppState::new_for_tests(pool))).unwrap()
}

#[tokio::test]
async fn metrics_series_filters_by_project_and_includes_metrics_and_fingerprint() {
    let pool = test_pool().await;
    seed_three_sessions(&pool).await;
    let s = server(pool);
    let r = s.get("/v1/metrics").add_query_param("project", "/proj/A").await;
    r.assert_status_ok();
    let body: Value = r.json();
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    // 최신 우선 정렬
    assert_eq!(sessions[0]["session_id"], "sess_a_new");
    assert_eq!(sessions[0]["metrics"]["tool_call_total"].as_i64().unwrap(), 2);
    assert_eq!(sessions[0]["fingerprint"]["cwds"][0], "/proj/A");
    assert_eq!(body["data"]["session_count"].as_i64().unwrap(), 2);
    assert_eq!(body["data"]["matched_count"].as_i64().unwrap(), 2);
    // F1: rate 필드 금지
    assert!(sessions[0]["metrics"].get("tool_failure_rate").is_none());
}

#[tokio::test]
async fn metrics_series_filters_by_time_window() {
    let pool = test_pool().await;
    seed_three_sessions(&pool).await;
    let s = server(pool);
    let r = s
        .get("/v1/metrics")
        .add_query_param("project", "/proj/A")
        .add_query_param("from", "2026-06-05T00:00:00Z")
        .await;
    let body: Value = r.json();
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1, "from 이후 first_observed 세션만");
    assert_eq!(sessions[0]["session_id"], "sess_a_new");

    let r2 = s
        .get("/v1/metrics")
        .add_query_param("to", "2026-06-04T00:00:00Z")
        .await;
    let body2: Value = r2.json();
    let s2 = body2["data"]["sessions"].as_array().unwrap();
    assert_eq!(s2.len(), 1);
    assert_eq!(s2[0]["session_id"], "sess_a_old");
}

#[tokio::test]
async fn metrics_series_rejects_unknown_params_and_bad_time() {
    let pool = test_pool().await;
    let s = server(pool);
    let r = s.get("/v1/metrics").add_query_param("nope", "1").await;
    r.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let r2 = s.get("/v1/metrics").add_query_param("from", "yesterday").await;
    r2.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn metrics_series_limit_truncates_but_reports_matched() {
    let pool = test_pool().await;
    seed_three_sessions(&pool).await;
    let s = server(pool);
    let r = s.get("/v1/metrics").add_query_param("limit", "1").await;
    let body: Value = r.json();
    assert_eq!(body["data"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["session_count"].as_i64().unwrap(), 1);
    assert_eq!(body["data"]["matched_count"].as_i64().unwrap(), 3);
}
```

- [ ] **Step 2: 빨강 확인** — Run: `cargo test --test api_metrics_sessions 2>&1 | tail -3` / Expected: 404/컴파일 실패

- [ ] **Step 3: series 수집기 구현**

`src/insight/series.rs`:

```rust
//! 세션 횡단 metrics series — HTTP `/v1/metrics`와 MCP `get_project_metrics`의
//! 공용 수집기. "프로젝트 P의 세션들에 대해 지표 추이/전후 비교"의 측정면이다.
//! 판단(개선됐는가)은 소비자(LLM) 몫 — 여기는 결정론 count와 fingerprint만.
//!
//! 구현: list_sessions_filtered(최신순) → first_observed_at 기간 필터 →
//! limit 절단 → 세션별 compute_session_metrics + compute_session_fingerprint.
//! 로컬 SQLite on-demand 패턴(§10.1) — 세션 수십 개 규모에서 충분.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::db::repo_observed;
use crate::error::Result;
use crate::insight::fingerprint::{compute_session_fingerprint, SessionFingerprint};
use crate::insight::metrics::{compute_session_metrics, SessionMetrics};

pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;
/// 후보 세션 수집 상한 — /v1/sessions의 5000 cap과 동일.
const CANDIDATE_CAP: i64 = 5000;

#[derive(Debug, Serialize)]
pub struct SessionSeriesRow {
    pub session_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub event_count: i64,
    pub metrics: SessionMetrics,
    pub fingerprint: SessionFingerprint,
}

#[derive(Debug, Serialize)]
pub struct SessionSeries {
    pub sessions: Vec<SessionSeriesRow>,
    /// 반환된 세션 수 (= sessions.len()).
    pub session_count: i64,
    /// limit 절단 전 필터 일치 세션 수 — 절단 사실을 숨기지 않는다.
    pub matched_count: i64,
}

pub async fn collect(
    pool: &SqlitePool,
    project: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<SessionSeries> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let rows = repo_observed::list_sessions_filtered(pool, CANDIDATE_CAP, project).await?;
    let in_window = |first: &str| -> bool {
        let Ok(ts) = DateTime::parse_from_rfc3339(first) else {
            // 저장 형식은 to_rfc3339 — 파싱 불가 행은 보수적으로 포함하지 않는다.
            return false;
        };
        let ts = ts.with_timezone(&Utc);
        from.is_none_or(|f| ts >= f) && to.is_none_or(|t| ts <= t)
    };
    let matched: Vec<_> = rows
        .into_iter()
        .filter(|r| in_window(&r.first_observed_at))
        .collect();
    let matched_count = matched.len() as i64;
    let mut sessions = Vec::new();
    for r in matched.into_iter().take(limit as usize) {
        let metrics = compute_session_metrics(pool, &r.session_id).await?;
        let fingerprint = compute_session_fingerprint(pool, &r.session_id).await?;
        sessions.push(SessionSeriesRow {
            session_id: r.session_id,
            first_observed_at: r.first_observed_at,
            last_observed_at: r.last_observed_at,
            event_count: r.event_count,
            metrics,
            fingerprint,
        });
    }
    Ok(SessionSeries {
        session_count: sessions.len() as i64,
        sessions,
        matched_count,
    })
}
```

(주의: `is_none_or`는 Rust 1.82+ — toolchain이 낮으면 `map_or(true, …)`로. 실행 시 확인.)

`src/insight/mod.rs`에 `pub mod series;` 추가.

- [ ] **Step 4: 핸들러 + 라우트**

`src/api/routes.rs`:

```rust
/// `GET /v1/metrics` — 세션 횡단 metrics+fingerprint series.
/// 미지원 파라미터는 400(deny_unknown_fields), from/to는 RFC3339.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsSeriesQuery {
    pub project: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

pub async fn metrics_series(
    State(pool): State<SqlitePool>,
    Query(q): Query<MetricsSeriesQuery>,
) -> impl IntoResponse {
    fn parse_time(
        s: Option<&str>,
        name: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, (StatusCode, Json<serde_json::Value>)> {
        match s {
            None => Ok(None),
            Some(v) => chrono::DateTime::parse_from_rfc3339(v)
                .map(|d| Some(d.with_timezone(&chrono::Utc)))
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "type": "about:blank",
                            "title": "INVALID_TIME",
                            "detail": format!("{name} must be RFC3339"),
                        })),
                    )
                }),
        }
    }
    let from = match parse_time(q.from.as_deref(), "from") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let to = match parse_time(q.to.as_deref(), "to") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let project_norm = q
        .project
        .as_deref()
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| !p.is_empty())
        .map(String::from);
    let limit = q.limit.unwrap_or(crate::insight::series::DEFAULT_LIMIT);
    match crate::insight::series::collect(&pool, project_norm.as_deref(), from, to, limit).await {
        Ok(series) => {
            Json(Envelope { meta: ResponseMeta::now(), data: series }).into_response()
        }
        Err(err) => {
            tracing::error!(err = %err, "metrics_series failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}
```

`src/api/mod.rs`: `.route("/v1/metrics", get(routes::metrics_series))`

- [ ] **Step 5: 통과 확인** — Run: `cargo test --test api_metrics_sessions` / Expected: PASS (4 tests)

- [ ] **Step 6: Commit**

```bash
git add src/insight/series.rs src/insight/mod.rs src/api/routes.rs src/api/mod.rs tests/api_metrics_sessions.rs
git commit -m "feat(api): GET /v1/metrics 세션 횡단 metrics+fingerprint series"
```

---

### Task 6: MCP 도구 get_project_metrics

**Files:**
- Create: `src/api/mcp/tools/get_project_metrics.rs`
- Modify: `src/api/mcp/tools/mod.rs` (schema·tools_list·dispatch), `tests/mcp_tools_list.rs` (5→6), `tests/fixtures/mcp/tools_list_expected.json` (golden fixture — 같은 커밋 필수)
- Test: `tests/mcp_tools_call.rs`에 케이스 추가

- [ ] **Step 1: 실패하는 테스트** — `tests/mcp_tools_list.rs`의 count를 6으로, required에 `whats_in_my_cc.get_project_metrics` 추가. `tests/mcp_tools_call.rs`에 (기존 케이스 패턴 복사):

```rust
#[tokio::test]
async fn get_project_metrics_returns_series() {
    let server = make_server().await; // 파일 내 기존 헬퍼 재사용
    let sid = init_session(&server).await;
    let r = server
        .post("/mcp")
        .content_type("application/json")
        .add_header(
            axum::http::HeaderName::from_static("mcp-session-id"),
            axum::http::HeaderValue::from_str(&sid).unwrap(),
        )
        .json(&json!({
            "jsonrpc": "2.0", "id": 9, "method": "tools/call",
            "params": {"name": "whats_in_my_cc.get_project_metrics", "arguments": {"limit": 5}}
        }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let payload: Value = serde_json::from_str(text).unwrap();
    assert!(payload["data"]["sessions"].is_array());
    assert!(payload["data"]["matched_count"].is_i64());
}
```

- [ ] **Step 2: 빨강 확인** — Run: `cargo test --test mcp_tools_list --test mcp_tools_call 2>&1 | tail -5` / Expected: FAIL

- [ ] **Step 3: 구현**

`src/api/mcp/tools/get_project_metrics.rs`:

```rust
//! MCP 도구: whats_in_my_cc.get_project_metrics — `/v1/metrics`와 동일 series.
//! 회고 스킬의 전후 비교(개입 효과 귀속)가 단일 툴콜로 끝나게 한다.

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::api::mcp::tools::{tool_error, tool_success};
use crate::insight::series;

pub async fn call(args: &Value, pool: &SqlitePool) -> Value {
    let project = args["project"]
        .as_str()
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty());
    let parse_time = |key: &str| -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
        match args[key].as_str() {
            None => Ok(None),
            Some(v) => chrono::DateTime::parse_from_rfc3339(v)
                .map(|d| Some(d.with_timezone(&chrono::Utc)))
                .map_err(|_| format!("{key} must be RFC3339")),
        }
    };
    let from = match parse_time("from") {
        Ok(v) => v,
        Err(e) => return tool_error(e),
    };
    let to = match parse_time("to") {
        Ok(v) => v,
        Err(e) => return tool_error(e),
    };
    let limit = args["limit"].as_i64().unwrap_or(series::DEFAULT_LIMIT);
    match series::collect(pool, project.as_deref(), from, to, limit).await {
        Ok(s) => match serde_json::to_value(&s) {
            Ok(data) => tool_success(json!({ "data": data })),
            Err(e) => tool_error(format!("serialize error: {e}")),
        },
        Err(e) => tool_error(format!("db error: {e}")),
    }
}
```

`mod.rs`: `pub mod get_project_metrics;` + schema fn + dispatch arm + tools_list entry:

```rust
fn get_project_metrics_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": { "type": "string", "description": "Project root path filter (same as search_sessions.project)" },
            "from": { "type": "string", "description": "RFC3339 — only sessions whose first observed event is at or after this time" },
            "to": { "type": "string", "description": "RFC3339 — only sessions whose first observed event is at or before this time" },
            "limit": { "type": "integer", "default": 50, "description": "Max sessions returned (newest first, max 200)" }
        },
        "required": []
    })
}
```

tools_list entry description (판단 배제 framing):

```rust
            {
                "name": "whats_in_my_cc.get_project_metrics",
                "description": "Cross-session deterministic metrics series: per-session behavioral counts plus environment fingerprint (models, cc_versions, git_branches, claude_md instruction hashes). Group sessions by fingerprint to compare before/after a harness or prompt change. Counts only — rates and judgments are the caller's.",
                "inputSchema": get_project_metrics_schema()
            }
```

`tests/fixtures/mcp/tools_list_expected.json`에 같은 entry 추가(이름 정렬 무관 — 테스트가 정렬 비교).

- [ ] **Step 4: 통과 확인** — Run: `cargo test --test mcp_tools_list --test mcp_tools_call` / Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/api/mcp/tools/ tests/mcp_tools_list.rs tests/mcp_tools_call.rs tests/fixtures/mcp/tools_list_expected.json
git commit -m "feat(mcp): get_project_metrics 도구 — 세션 횡단 series"
```

---

### Task 7: DetectorManifest metric_class (process|outcome)

**Files:**
- Modify: `src/insight/manifest.rs`, 5개 extractor (`src/insight/extractors/{tool_failure,risky_action,context_bloat,re_read,final_state_mismatch}.rs`)
- Test: `tests/detector_manifest.rs`에 단언 추가

- [ ] **Step 1: 실패하는 테스트** — `tests/detector_manifest.rs`에 추가 (기존 manifest 순회 패턴 재사용):

```rust
#[test]
fn every_manifest_declares_metric_class() {
    for m in wimcc::insight::pipeline::detector_manifests() {
        assert!(
            m.metric_class == "process" || m.metric_class == "outcome",
            "{}: metric_class must be process|outcome",
            m.id
        );
    }
}

#[test]
fn final_state_mismatch_is_outcome_class() {
    let m = wimcc::insight::pipeline::detector_manifests()
        .into_iter()
        .find(|m| m.id == "final_state_mismatch")
        .unwrap();
    assert_eq!(m.metric_class, "outcome");
}
```

(`detector_manifests()` 공개 함수명은 실행 시 pipeline.rs:105 부근에서 확인 — `pub(crate)`라면 테스트는 `/v1/detectors` HTTP 응답으로 단언하는 형태로 바꾼다.)

- [ ] **Step 2: 빨강 확인** — Run: `cargo test --test detector_manifest 2>&1 | tail -3` / Expected: COMPILE FAIL

- [ ] **Step 3: 구현** — `DetectorManifest`에 필드 추가:

```rust
    /// Goodhart 가드 메타데이터: 이 detector의 신호가 과정 지표(process —
    /// 행동 형태, 회피·게임 가능)인지 결과 지표(outcome — 최종 상태에 결부,
    /// 게임 난도 높음)인지. 분류 기준: verification/최종 상태를 읽으면 outcome.
    pub metric_class: &'static str,
```

각 extractor `manifest()`: `final_state_mismatch` → `"outcome"`, 나머지 4종 → `"process"`.

- [ ] **Step 4: 통과 확인** — Run: `cargo test --test detector_manifest && cargo test --test api_detectors` / Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/insight/manifest.rs src/insight/extractors/ tests/detector_manifest.rs
git commit -m "feat(insight): detector manifest에 metric_class(process|outcome)"
```

---

### Task 8: retrospect 스킬 — 가설 원장 + 전후 비교 (doc-only, TDD 예외)

**Files:**
- Modify: `skills/session-retrospect/SKILL.md`, `skills/session-retrospect/references/workflow.md`

- [ ] **Step 1: SKILL.md 개정** — 변경점:
  1. Step 1 뒤에 **Step 1.5 — 이전 회고 원장 로드**: `ls <프로젝트>/dogfood/*-retrospect*.md` → 최신 문서들의 제안 표(ID·예측 지표) 읽기. 제안별 채택 여부는 `git log --oneline -S"<제안 핵심 문자열>"` 또는 해당 파일 diff로 판정. 원장이 없으면 첫 회고로 진행.
  2. Step 2 수집 목록에 `GET /v1/sessions/:id/fingerprint` 추가.
  3. **Step 5 — 전후 비교 (채택된 제안이 있을 때)**: MCP `whats_in_my_cc.get_project_metrics {project}` (HTTP fallback `GET /v1/metrics?project=`)로 series를 받아, 채택 커밋 시각 또는 `fingerprint.instruction_sha256`/`claude_md` 해시 변화로 전/후 코호트를 나누고 **예측했던 지표**를 비교한다. 규칙: 표본 수 명시, 혼재 요인(`models`·`cc_versions` 차이) 확인, process 지표만 좋아지고 outcome 지표(verification 계열)가 정체·악화하면 지표 게임 가능성을 함께 보고.
  4. Step 4 산출 형식: 제안마다 안정 ID `R-<YYYYMMDD>-<n>` + **예측** 컬럼(개선될 지표 / 악화 가능 지표) 의무화. dogfood 저장을 "(선택)"에서 **기본**으로 격상 — 원장이 없으면 다음 회고가 비교할 수 없다.
- [ ] **Step 2: workflow.md 개정** — 추가 절:
  - "전후 비교 절차": get_project_metrics 호출 예시, 코호트 분할 기준 우선순위(① instruction_sha256 변화 ② 채택 커밋 시각 ③ 수동 지정), 비교 보고 형식(전/후 세션 수, count 합·중앙값, 단정 금지 문구).
  - "Goodhart 주의": `/v1/detectors`의 `metric_class` 활용 — process 개선 주장에는 outcome 동반 확인.
  - "판별→fixture 승격": 회고에서 확정된 패턴 사례를 `tests/fixtures/**/real/`에 동결 + invariant 테스트로 잠그는 것이 detector 후보(`re_edit_churn` 등) 졸업의 공식 관문임을 명문화 (Real-data anchoring의 확장).
  - 치트시트 표에 fingerprint·get_project_metrics 행 추가.
- [ ] **Step 3: Commit**

```bash
git add skills/session-retrospect/
git commit -m "feat(skill): retrospect 가설 원장(제안 ID·예측 지표)과 전후 비교 단계"
```

---

### Task 9: 사양·문서 현행화 (doc-only, TDD 예외)

**Files:**
- Modify: `docs/04_api_mcp_spec.html` (endpoint 표 +2행, MCP tools 표 +1행)
- Modify: `docs/00_prd_revised.html` (§04 Users 행 추가 — "회고 LLM(frontier judge)" / job: 결정론 집계로 개입 전후 비교·판별 / success: fingerprint+series를 소수 호출로; SRV-2 문구에 fingerprint·cross-session metrics 추가; SRV-3 도구 목록 갱신 4종→6종)
- Modify: `docs/superpowers/specs/2026-05-27-witmcc-ux-redesign-epic.md` (§3 앞에 2026-06-12 Revision note: C1/C3~C7의 findings·episodes·graph·L2 judge 전제는 Signal 모델 전환으로 폐기 — 현행 계약(signals/evidence_refs·verification covers·turns·metrics·fingerprint·series)으로 대체 표 제시)
- Modify: `docs/implementation-notes.html` (새 섹션 `#self-improvement-loop-2026-06-12`: 설계 결정 — capture-on-receipt·해시만 저장·과거 세션 소급 불가 실측 근거·on-demand fingerprint/series·metric_class·스킬 원장; 편차/트레이드오프/열린 질문 포함)
- Modify: `CLAUDE.md` (Status 항목 추가 + epic 파일명 오기 정정 `wimcc-ux-redesign-epic.md` → `witmcc-ux-redesign-epic.md`)

- [ ] **Step 1: 04 spec 표 갱신** — "위 표가 전부다" 계약 유지: `/v1/sessions/:id/fingerprint`, `/v1/metrics`(쿼리 파라미터·400 의미론 포함) 행과 `get_project_metrics` 도구 행을 기존 마크업 그대로 추가.
- [ ] **Step 2: PRD 갱신** (위 명세대로 — 루프 KPI가 아니라 **판단자의 데이터 계약**으로 서술).
- [ ] **Step 3: epic charter Revision note + 대체 표.**
- [ ] **Step 4: implementation-notes 새 섹션.**
- [ ] **Step 5: CLAUDE.md Status 현행화.**
- [ ] **Step 6: Commit**

```bash
git add docs/ CLAUDE.md
git commit -m "docs: 자기개선 루프 표면 사양 현행화 (PRD 소비자 계약·04 spec·epic 정정·impl-notes)"
```

---

### Task 10: 전체 게이트 + PR

- [ ] **Step 1:** `cargo fmt --all -- --check` → 위반 시 `cargo fmt --all`
- [ ] **Step 2:** `cargo clippy --all-targets -- -D warnings` → 0 warnings
- [ ] **Step 3:** `cargo test` 전체 green (webui 미변경 — vitest 불요, CI가 재검증)
- [ ] **Step 4:** Self-check 체크리스트(CLAUDE.md) 5항목 점검 — 특히 "표본 1 일반화 금지" 문구가 새 문서에 지켜졌는지.
- [ ] **Step 5:** `git push -u origin feat/self-improvement-loop-foundations` 후 `gh pr create` — 본문에 리뷰 발견→구현 매핑, **AI footer 금지**(리포 훅이 차단).

---

---

## 확장 (2026-06-12 사용자 지시): Task 11–16 — 태그 사전 webui→core 이전 (발견 5)

> 사용자 지시로 같은 PR에 포함. 원칙: **분류(측정)는 Rust core가 소유**, webui는
> 서버 값을 소비(파서·사전 전부 삭제), 표현(verb 색상·hint 문구)만 frontend 유지.

### Task 11: Rust 분류기 `src/insight/event_tags.rs` (TDD — TS 테스트 1:1 이식)

- Test: `tests/event_tags.rs` — `webui/.../eventTags.test.ts`의 분류 케이스 전수 이식
  (taxonomy·compound·global-flag·timeout·continuation·newline·redirect·assignment·
  loop-keyword·comment·Read/Write ext·control/unmatched·no-slash-key invariant·
  meaningful display·untagged token 집계 의미론).
- 공개 API:
  ```rust
  pub enum TagDisposition { Tagged, Control, Unmatched } // as_str(): "tagged"|"control"|"unmatched"
  pub struct TagOutcome {
      pub value: Option<&'static str>,   // "read.file" 등 verb.object — 21종
      pub disposition: TagDisposition,
      pub token: Option<String>,         // untagged-loop 집계 키 (Bash 첫 토큰 | "tool sub" | ext | basename)
      pub display: Option<String>,       // meaningful command(선행 제어 세그먼트 제거) | file_path
  }
  pub fn classify_tool_call(tool_name: Option<&str>, payload: &serde_json::Value) -> TagOutcome
  pub fn segment_command(cmd: &str) -> Vec<String>      // 테스트 패리티용 공개
  pub fn meaningful_command(cmd: &str) -> String
  ```
- RED: 모듈 부재 컴파일 실패 확인 → GREEN: TS 구현 1:1 이식(사전 4종 + 파서) → Commit
  `feat(insight): event tag 분류기를 core로 이전 (webui eventTags 패리티)`.

### Task 12: events 응답에 `tag` 노출

- Test: `tests/api_events_tag.rs` — Bash tool_call 시드 → `/v1/sessions/:id/events` 응답
  이벤트에 `tag.value=="read.file"`·`tag.disposition=="tagged"` / 비 tool_call 이벤트는 `tag: null`.
- 구현: `routes.rs::observed_to_dto`에 `"tag": (kind==ToolCall).then(|| classify…)` 추가.
- Commit `feat(api): 렌더 이벤트에 tag(value·disposition·token·display) 노출`.

### Task 13: turns rollup에 `tag_histogram`

- Test: `tests/api_session_turns.rs`(기존 파일) 또는 turn_rollup 단위 — Bash grep 2회 시드 →
  turn의 `tag_histogram == {"read.file": 2}`.
- 구현: `turn_rollup.rs` ToolCall arm에서 `classify_tool_call` 호출, value 있는 것만 카운트.
- Commit `feat(insight): turns rollup에 tag_histogram`.

### Task 14: webui — 서버 태그 소비로 전환

- `api/types.ts`: `EventTagDto { value, disposition, token, display }` + `ObservedEventDto.tag?`.
- `eventTags.ts`: 사전·파서·tagForEvent·meaningfulCommand·segmentCommand **삭제**.
  잔존: `Tag`/`TagVerb`/`tagVerb`(칩 색), `UntaggedRow`, `collectUntagged`(서버 필드 기반:
  `disposition==='unmatched'` 그룹핑 by `token`, hint는 token 형태+tool_name으로
  `src/insight/event_tags.rs`의 맵 이름 안내), sample은 `display.slice(0,80)`.
- `ActivityStack.tsx`: `e.tag` 사용. `nodeLabel.ts`: `e.tag?.display ?? raw`. `UntaggedBashPanel` 무변경.
- `eventTags.test.ts`: 분류 케이스 삭제(→Rust), 잔존 기능(tagVerb·collectUntagged 그룹핑·hint) 테스트로 재작성.
- `scripts/untagged-bash.ts`: collectUntagged 시그니처 유지 확인.
- 검증: `cd webui && npx vitest run` green. Commit `feat(webui): 이벤트 태그를 서버 값으로 소비 (사전 제거)`.

### Task 15: 통합 검증 — dist 재빌드 + 브라우저 smoke (CLAUDE.md 의무)

- `cd webui && npm run build` → `cargo build --release` → serve 재기동 →
  claude-in-chrome으로 replay 화면 진입, 태그 칩 렌더·Untagged 패널 동작 시각 확인.

### Task 16: 문서 + PR 갱신

- 04 spec: events 행에 `tag` 필드, turns 행에 `tag_histogram` 추가.
- CLAUDE.md Tagging loop 절: 규칙 위치를 `src/insight/event_tags.rs`로, 테스트 잠금을 Rust로 갱신.
- impl-notes `#self-improvement-loop-2026-06-12`에 태그 이전 bullet 추가.
- `gh pr edit`로 PR 본문의 "범위 외" 항목 정정. Commit `docs: 태그 어휘 core 이전 반영`.

---

## Self-Review 결과

- 범위: 리뷰 발견 1·2·3·6·7 + 우선순위 0을 Task 1–9가 커버. 발견 5(태그 사전 core 이전)는 Task 11–16으로 **이 PR에 포함** (2026-06-12 사용자 지시로 확장 — 종전 "별도 PR 분리" 결정을 대체).
- 타입 일관성: `InstructionFile`(Task 1)을 Task 3이 재사용, `SessionSeries`(Task 5)를 Task 6이 재사용 — 시그니처 일치 확인.
- 실행 시 확인 표식: pre_tool_use fixture의 session_id(Task 2), `detector_manifests()` 가시성(Task 7), `is_none_or` toolchain(Task 5), repo_observed payload 직렬화 형식(Task 3).
