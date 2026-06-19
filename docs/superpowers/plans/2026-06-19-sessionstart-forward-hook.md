# SessionStart forward hook (플러그인 번들) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** ❌ 미채택 — 구현·smoke까지 마쳤으나 효용 검증 실패로 되돌림(`.mcp.json` 포트 동적화만 채택). 근거는 design doc·implementation-notes.

**Goal:** `session-retrospect` 플러그인을 설치하면 별도 settings 편집 없이 SessionStart hook이 wimcc로 forward되어, fingerprint 자기개선 독립변수(`claude_md`/`instruction_sha256`)의 유일 소스인 instruction_snapshot을 수집한다.

**Architecture:** 플러그인 루트에 `hooks/hooks.json`을 추가해 SessionStart에 인라인 `command` curl forward를 건다. 같은 플러그인의 `.mcp.json` url과 함께 `${WIMCC_PORT:-7878}`로 포트를 동적화한다. wimcc 백엔드(`/hooks/v1/events` 수신, `instruction_snapshot` 캡처, `fingerprint` 소비)는 이미 구현돼 있어 Rust 변경은 없다.

**Tech Stack:** Claude Code 플러그인(hooks.json / .mcp.json), curl, jq(검증), wimcc serve(SQLite).

## Global Constraints

- 포트는 항상 `${WIMCC_PORT:-7878}` — **기본값 `:-7878` 필수**(default 없는 미설정 var는 `.mcp.json` 파싱 실패: code.claude.com/docs/mcp).
- fail-soft(PRD OBS-3): forward 명령은 `-m 2` + `|| true`로 끝내 wimcc 다운·지연에도 세션을 막지 않는다.
- SessionStart**만** 등록. 타 lifecycle은 transcript 중복이라 추가하지 않는다.
- 플러그인 hook은 `plugins/session-retrospect/hooks/hooks.json` (plugin.json에 직접 X).
- 커밋 메시지에 AI 푸터(Co-Authored-By / Generated) 금지.
- Rust/WebUI 코드 변경 없음 — 단위 테스트 없이 jq 유효성 + 실환경 smoke로 검증.

---

### Task 1: 플러그인 SessionStart forward hook 추가

**Files:**
- Create: `plugins/session-retrospect/hooks/hooks.json`

**Interfaces:**
- Produces: SessionStart 시 `POST http://127.0.0.1:${WIMCC_PORT:-7878}/hooks/v1/events`로 hook input JSON(stdin) forward. wimcc `src/ingest/hook.rs`가 이를 수신(`hook_event_name=SessionStart` 요구).

- [ ] **Step 1: `hooks/hooks.json` 작성**

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume|clear|compact",
        "hooks": [
          {
            "type": "command",
            "command": "curl -sS -m 2 -X POST -H 'content-type: application/json' --data-binary @- \"http://127.0.0.1:${WIMCC_PORT:-7878}/hooks/v1/events\" >/dev/null 2>&1 || true"
          }
        ]
      }
    ]
  }
}
```

- [ ] **Step 2: JSON 유효성 + 구조 검증**

Run:
```bash
jq -e '.hooks.SessionStart[0].hooks[0].command | test("hooks/v1/events") and test("WIMCC_PORT") and test("\\|\\| true")' plugins/session-retrospect/hooks/hooks.json
```
Expected: `true` (출력) + exit 0. JSON 파싱 실패 시 jq가 에러.

- [ ] **Step 3: 커밋**

```bash
git add plugins/session-retrospect/hooks/hooks.json
git commit -m "feat(session-retrospect): SessionStart forward hook 번들

플러그인 설치 시 SessionStart instruction_snapshot을 wimcc로 forward.
포트는 \${WIMCC_PORT:-7878}, fail-soft(-m 2 || true)."
```

---

### Task 2: `.mcp.json` 포트 동적화

**Files:**
- Modify: `plugins/session-retrospect/.mcp.json`

**Interfaces:**
- Consumes: env `WIMCC_PORT`(Task 1과 동일 변수 공유).
- Produces: MCP http 연결 url이 hook과 같은 포트를 따라감.

- [ ] **Step 1: `.mcp.json`의 url을 동적 포트로 교체**

파일 전체를 다음으로 교체:
```json
{
  "mcpServers": {
    "wimcc": {
      "type": "http",
      "url": "http://127.0.0.1:${WIMCC_PORT:-7878}/mcp"
    }
  }
}
```

- [ ] **Step 2: 유효성 + 기본값 보존 검증**

Run:
```bash
jq -e '.mcpServers.wimcc.url == "http://127.0.0.1:${WIMCC_PORT:-7878}/mcp"' plugins/session-retrospect/.mcp.json
```
Expected: `true` + exit 0. (`:-7878` 기본값이 있어 미설정 시에도 기존 7878 동작 유지 — default 누락 시 파싱 실패하므로 이 검증이 그것까지 막는다.)

- [ ] **Step 3: 커밋**

```bash
git add plugins/session-retrospect/.mcp.json
git commit -m "feat(session-retrospect): MCP url 포트를 WIMCC_PORT로 동적화

hook과 동일 env(WIMCC_PORT) 공유 — 스모크 시 포트 변경에 함께 대응.
기본값 :-7878로 기존 동작 유지."
```

---

### Task 3: README ×2 + implementation-notes 갱신

**Files:**
- Modify: `README.ko.md` (Hooks 절, 현재 약 212–237행)
- Modify: `README.md` (대응 Hooks 절)
- Modify: `docs/implementation-notes.html`

**Interfaces:** 없음(문서).

- [ ] **Step 1: `README.ko.md`의 Hooks 절 교체**

기존 "forward 스크립트를 9개 lifecycle에 수동 등록 + `/usr/local/bin/wimcc-forward.sh`" 안내 블록 전체를 다음으로 교체:

```markdown
### Hooks (자기개선 fingerprint)

`session-retrospect` 플러그인을 설치하면 **SessionStart hook이 자동 등록**되어, 세션
시작 시점의 CLAUDE.md 스냅샷(`instruction_snapshot`)을 wimcc로 forward한다 — 자기개선
코호트 분석(`fingerprint`)의 독립변수(`claude_md`/`instruction_sha256`)다. transcript에는
CLAUDE.md가 남지 않으므로 이 경로가 유일한 관측 수단이다. **별도 `settings.json` 편집은
필요 없다.**

wimcc를 기본(`7878`)과 다른 포트로 띄우면 `WIMCC_PORT` 환경변수로 지정한다 — hook과 MCP
연결이 함께 그 포트를 따라간다(예: `WIMCC_PORT=9000`).

forward는 fail-soft다(`-m 2` + `|| true`) — wimcc가 죽었거나 느려도 세션은 막히지 않는다.
PreToolUse/PostToolUse 등 다른 lifecycle은 transcript live tail로 이미 수집되므로 수동
forward 등록은 불필요하다(중복).
```

- [ ] **Step 2: `README.md`(영문)의 대응 Hooks 절을 같은 내용으로 교체**

먼저 `README.md`에서 hook/forward 안내 위치를 확인:
```bash
grep -n 'hooks/v1/events\|wimcc-forward\|SessionStart\|## Hooks\|### Hooks' README.md
```
해당 블록을 위 한국어 절의 영어 번역으로 교체(동일 요지: 플러그인 설치 시 SessionStart 자동, `WIMCC_PORT`, fail-soft, 타 lifecycle 불필요).

- [ ] **Step 3: `docs/implementation-notes.html`에 결정 기록 추가**

기존 항목 형식을 따라(먼저 파일에서 유사 섹션 패턴 확인: `grep -n '<h[23]' docs/implementation-notes.html | tail`), hook/플러그인 관련 위치에 다음 요지의 단락을 추가:
- collector forward는 플러그인(`session-retrospect/hooks/hooks.json`)이 SessionStart만 번들. 타 lifecycle은 transcript 중복이라 제외.
- **Non-goals 정합성**: 플러그인이 자기 hook을 번들하는 것은 wimcc가 사용자 `settings.json`을 변경하는 것이 아니라, 사용자의 설치 선택으로 활성화되는 표준 플러그인 메커니즘 — "wimcc는 settings를 자동 수정하지 않는다" 원칙과 충돌하지 않음.
- hook·MCP url 모두 `${WIMCC_PORT:-7878}` 공유.
- instruction_snapshot은 fingerprint 자기개선 독립변수의 유일 소스(transcript엔 CLAUDE.md 미기록, 2026-06-12 실측).

- [ ] **Step 4: 커밋**

```bash
git add README.ko.md README.md docs/implementation-notes.html
git commit -m "docs: 플러그인 SessionStart hook 연동 안내로 README/notes 갱신

수동 9개 lifecycle 등록 안내를 플러그인 자동 SessionStart로 교체.
WIMCC_PORT·fail-soft·타 lifecycle 중복 비권장 명시."
```

---

### Task 4: end-to-end smoke 검증 (실환경)

**Files:** 없음(검증 전용). 자동화 불가 — 사용자 환경에서 실제 새 세션이 필요.

**Interfaces:**
- Consumes: Task 1·2의 플러그인 변경, `wimcc serve` 가동.
- 열린 질문 두 개를 여기서 닫는다: (a) SessionStart matcher의 파이프(`startup|resume|clear|compact`) 지원 여부, (b) SessionStart payload의 `cwd` 포함 여부.

- [ ] **Step 1: wimcc serve 가동 + 플러그인 활성화**

```bash
# 별도 포트로 테스트하려면:
export WIMCC_PORT=7878   # 또는 다른 포트로 띄웠으면 그 값
wimcc serve --auto-migrate    # 백그라운드 또는 별도 터미널
```
플러그인이 enabled인지 확인(`session-retrospect@wimcc`). 세션 중 토글했다면 `/reload-plugins` 또는 재시작.

- [ ] **Step 2: 새 세션 시작 → forward 발동 확인**

`export WIMCC_PORT=<port>`(serve와 동일)한 셸에서 **새 claude 세션**을 시작한다(도중 등록은 이미 지나간 SessionStart를 못 잡음). 그 후 DB에서 collector SessionStart 수신 확인:
```bash
sqlite3 .wimcc.sqlite "SELECT count(*) FROM observed_event o JOIN raw_event r ON o.raw_event_id=r.raw_event_id WHERE r.source_type='hook' AND o.subkind='session_start';"
```
Expected: ≥ 1. (0이면 matcher 파이프 미지원 의심 → Step 4 fallback.)

- [ ] **Step 3: instruction_snapshot + fingerprint 채워짐 확인**

```bash
echo "=== captured.claude_md 담은 event (이전 0 → ≥1 기대) ==="
sqlite3 .wimcc.sqlite "SELECT count(*) FROM observed_event WHERE json_extract(payload,'\$.captured.claude_md') IS NOT NULL;"
echo "=== 해당 세션 fingerprint claude_md/instruction_sha256 ==="
# 새 세션 id로 교체
curl -sS "http://127.0.0.1:${WIMCC_PORT:-7878}/v1/sessions" | jq -r '.data[0].session_id'
```
Expected: captured event ≥ 1. `cwd`가 payload에 없으면 `captured`가 안 생기므로(열린 질문 b), 0이면 SessionStart payload에 cwd 부재로 판단 → implementation-notes에 한계로 기록.

- [ ] **Step 4: (필요 시) matcher fallback**

Step 2가 0건이면 SessionStart matcher가 파이프를 안 받는 것. `hooks/hooks.json`의 SessionStart 배열을 4개 그룹으로 분리:
```json
"SessionStart": [
  { "matcher": "startup",  "hooks": [ { "type": "command", "command": "<동일 curl>" } ] },
  { "matcher": "resume",   "hooks": [ { "type": "command", "command": "<동일 curl>" } ] },
  { "matcher": "clear",    "hooks": [ { "type": "command", "command": "<동일 curl>" } ] },
  { "matcher": "compact",  "hooks": [ { "type": "command", "command": "<동일 curl>" } ] }
]
```
(`<동일 curl>`은 Task 1 Step 1의 command 문자열 그대로.) 재검증 후 결과를 implementation-notes에 반영, 커밋.

- [ ] **Step 5: 검증 결과를 implementation-notes에 확정 기록 + 커밋**

matcher 파이프 지원 여부, cwd 포함 여부(captured 생성 여부)를 실측 결과로 `docs/implementation-notes.html`에 기록.
```bash
git add docs/implementation-notes.html plugins/session-retrospect/hooks/hooks.json
git commit -m "docs: SessionStart hook smoke 결과(matcher/cwd) 확정 기록"
```

---

## Self-Review

**1. Spec coverage:**
- 배치(session-retrospect) → Task 1. ✓
- SessionStart만, command 인라인, fail-soft → Task 1. ✓
- 포트 `${WIMCC_PORT:-7878}` (hook + .mcp.json) → Task 1·2. ✓
- README/docs 갱신 → Task 3. ✓
- Non-goals 정합성 기록 → Task 3 Step 3. ✓
- 검증(end-to-end, fingerprint 0→1) → Task 4. ✓
- 열린 질문(matcher 파이프, cwd) → Task 4에서 close. ✓

**2. Placeholder scan:** Task 3 Step 2의 영문 번역과 Step 3의 HTML 단락은 정확한 위치 확인(grep)을 동반한 의도적 적응 — 요지·대상 파일·검증법이 명시됨. Task 4 Step 4 `<동일 curl>`은 Task 1 Step 1을 명시 참조(같은 문자열). 그 외 TBD/TODO 없음.

**3. Type consistency:** 전 task가 동일 키를 사용 — `WIMCC_PORT`(env), `source_type='hook'`, `subkind='session_start'`, `captured.claude_md`, `/hooks/v1/events`. 불일치 없음.
