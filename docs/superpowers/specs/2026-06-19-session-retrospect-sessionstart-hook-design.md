# session-retrospect 플러그인 SessionStart forward hook — 설계

- 날짜: 2026-06-19
- 상태: **미채택** — 구현·smoke 완료 후 되돌림. instruction_snapshot 순증분이 단일 사용자 환경에서 git 이력으로 대체 가능 + 코호트 분석이 표본·교란으로 신뢰성 약함. `.mcp.json` 포트 동적화(`${WIMCC_PORT:-7878}`)만 채택. 상세 근거는 implementation-notes.
- 영역: `plugins/session-retrospect`, hook collector(`/hooks/v1/events`), `src/insight/fingerprint.rs`, README

## 배경 / 문제

wimcc의 hook 데이터 출처는 두 갈래다.

1. **transcript live tail** — hook 실행 *결과*(`hook_success`/`hook_additional_context` 등)를 항상 수집. forward 불필요.
2. **hook collector(`/hooks/v1/events`)** — Claude Code lifecycle event 원본을 수신. settings.json에 forward hook을 등록해야만 들어옴.

대부분의 collector 신호(PreToolUse·PostToolUse·Stop의 tool_input/tool_response/prompt/last_assistant_message 등)는 transcript/OTel과 **중복**이며, 매 lifecycle마다 도는 forward 호출은 측정 부하만 만든다. 실측으로도 사용자가 본 collector 카드는 전부 중복이었다.

**유일한 예외가 SessionStart다.** `src/ingest/hook.rs`는 SessionStart collector hook 수신 시 그 시점의 CLAUDE.md를 `instruction_snapshot`으로 캡처해 `payload.captured.claude_md`에 저장한다. `src/insight/fingerprint.rs`는 이 값을 읽어 자기개선 루프의 독립변수(`claude_md` + 코호트 group key `instruction_sha256`)를 만든다. fingerprint 모듈 doc이 명시하듯 **transcript에는 CLAUDE.md가 기록되지 않으므로(2026-06-12 실측) SessionStart collector hook이 이 데이터의 유일한 관측 경로**다.

현 상태(실측): DB 전체에 `captured.claude_md`를 담은 observed_event는 **0건**. SessionStart부터 forward가 걸린 세션이 한 번도 없었기 때문이다. 즉 자기개선 트랙(CLAUDE.md `## Status` in-flight)의 독립변수가 한 번도 수집된 적이 없다.

## 목표

사용자가 settings.json을 직접 편집하지 않고 **플러그인 설치만으로** SessionStart forward가 동작하게 한다. 중복·낭비를 유발하는 다른 lifecycle은 등록하지 않는다.

## 비목표

- PreToolUse/PostToolUse 등 SessionStart 외 lifecycle forward (transcript 중복 — 수집하지 않음)
- 포트/엔드포인트 사용자 설정화 (YAGNI — 기본값 하드코딩)
- 기존 transcript 유래 `hook_event` 처리 변경 (그대로 유지)

## 설계

### 1. 배치

기존 `session-retrospect` 플러그인에 hook을 추가한다(별도 플러그인 신설 안 함). 이 플러그인은 이미 `wimcc serve`(127.0.0.1:7878)를 전제하는 wimcc 연동 플러그인이므로 의존성이 일치하고, 사용자는 플러그인 하나로 "수집(SessionStart forward) + 분석(회고 스킬)"을 모두 얻는다.

플러그인 hook은 플러그인 루트의 **`hooks/hooks.json`** 에 정의한다(plugin.json에 직접 넣지 않음 — Claude Code 플러그인 규약, code.claude.com/docs plugins). 플러그인이 enabled면 hook은 별도 설정 없이 자동 활성화된다(세션 중 토글 시 `/reload-plugins` 또는 재시작 필요할 수 있음).

### 2. hook 정의 (`plugins/session-retrospect/hooks/hooks.json`)

- event: **SessionStart만**. matcher는 모든 시작 유형(`startup`/`resume`/`clear`/`compact`)을 커버 — 각 세션이 어떤 CLAUDE.md로 시작했는지 빠짐없이 캡처. (collector는 `source_uri` 기준 dedup하므로 과수신해도 안전.)
- 타입: `command` 인라인.

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume|clear|compact",
        "hooks": [
          { "type": "command",
            "command": "curl -sS -m 2 -X POST -H 'content-type: application/json' --data-binary @- \"http://127.0.0.1:${WIMCC_PORT:-7878}/hooks/v1/events\" >/dev/null 2>&1 || true" }
        ]
      }
    ]
  }
}
```

- 포트: **`${WIMCC_PORT:-7878}`** — env `WIMCC_PORT`로 오버라이드, 미설정 시 wimcc 기본 7878. `command`는 shell 실행이라 그대로 동작. 스모크 테스트로 포트가 바뀌면 `export WIMCC_PORT=<port>` 한 셸에서 claude를 띄우면 hook이 따라간다. host/path는 고정(포트만 가변으로 충분).
- fail-soft(PRD OBS-3): `-m 2`(2초 상한) + `|| true`(실패 무시). wimcc 다운·지연에도 SessionStart가 세션을 막지 않는다. README에서 검증된 패턴.
- `http` 타입을 쓰지 않는 이유: wimcc 다운 시 Claude Code의 http hook 실패 처리(블로킹/에러 표시 여부)가 불확실해 OBS-3 보장이 어렵다. 인라인 `command` + `|| true`가 fail-soft를 명시적으로 보장한다.

### 2b. MCP 엔드포인트도 포트 동적화 (`plugins/session-retrospect/.mcp.json`)

같은 플러그인의 MCP url이 `127.0.0.1:7878` 하드코딩이라 스모크 시 hook과 함께 깨진다. `.mcp.json`은 `url` 필드에 환경변수 치환을 지원하므로(code.claude.com/docs/mcp) 동일 패턴으로 맞춘다:

```json
{
  "mcpServers": {
    "wimcc": { "type": "http", "url": "http://127.0.0.1:${WIMCC_PORT:-7878}/mcp" }
  }
}
```

- 기본값 `:-7878`이 있어 기존 동작은 그대로(미설정 시 7878). default 없는 미설정 var는 config 파싱 실패하므로 기본값 유지가 필수.
- hook과 MCP가 **같은 `WIMCC_PORT`** 를 공유 — 한 env로 둘 다 따라간다.

### 3. Non-goals 정합성

CLAUDE.md Non-goals("Claude Code 설정/hook 변경 금지")와 README("wimcc는 settings.json을 자동 수정하지 않는다")는 **여전히 유효**하다. 플러그인이 자기 hook을 번들하는 것은:

- 사용자 settings.json을 wimcc가 변경하는 것이 아니다.
- 사용자가 **플러그인 설치를 선택**하면 활성화되는 표준 Claude Code 플러그인 메커니즘이다(설치 = 동의).

따라서 "wimcc가 사용자 환경을 몰래 바꾸지 않는다"는 정신과 충돌하지 않는다. 이 구분을 `docs/implementation-notes.html`에 기록한다.

### 4. README / docs 갱신 (drift 방지)

- README.md / README.ko.md의 Hooks 절: "9개 lifecycle 수동 등록" 안내를 → **"플러그인 설치 시 SessionStart 자동 forward(자기개선 fingerprint용); PreToolUse 등 수동 등록은 transcript와 중복이라 비권장"** 으로 교체.
- `/hooks/v1/events` API 계약(`docs/04_api_mcp_spec.html`)은 **유지**(엔드포인트 자체는 그대로). 관측 대상 표의 hook 행도 유지.
- `docs/implementation-notes.html`에 본 설계의 결정·Non-goals 정합성·SessionStart 한정 근거를 기록.

## 검증

단위 테스트로 잠그기 어려운 통합 동작이므로 end-to-end smoke로 확인한다.

1. 플러그인 설치 후 **새 세션** 시작(SessionStart가 forward 등록 이후여야 함 — 도중 등록은 이미 지나간 SessionStart를 못 잡는다).
2. `wimcc serve` 수신: `POST /hooks/v1/events` 도착.
3. DB: `observed_event`에 `source_type=hook`, `subkind=session_start`, `payload.captured.claude_md` 존재.
4. `fingerprint`: 해당 세션의 `claude_md`/`instruction_sha256`이 채워짐(현재 0건 → 1건).

plugin.json/hooks.json 자체는 유효한 JSON·hook 스키마인지 검증(구현 시 `jq`/스키마 확인).

## 확정된 사실 (claude-code-guide + 공식 docs, 2026-06-19)

- hook 위치: `hooks/hooks.json` (plugin.json 직접 X). 설치 시 자동 활성화.
- `.mcp.json` url 환경변수 치환 지원(`${VAR:-default}`). default 없는 미설정 var는 파싱 실패.
- SessionStart matcher 값: `startup`/`resume`/`clear`/`compact`.

## 열린 질문 / 구현 시 확정

- SessionStart matcher가 `startup|resume|clear|compact` 파이프(정규식) 문법을 지원하는지 — docs 예시는 단일 값. 미지원이면 항목 4개로 분리하거나 matcher 생략(전체 매칭 가능 여부)으로 대체.
- SessionStart hook input payload에 `cwd`가 포함되는지 — `instruction_snapshot`은 `cwd` 없으면 캡처를 degrade(`captured` 키 미생성). 구현 후 smoke에서 `captured.claude_md` 생성 여부로 확인.
