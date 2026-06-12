---
name: session-retrospect
description: wimcc로 관측된 Claude Code 세션을 회고 분석해 그 프로젝트의 개선 제안(프롬프트·스킬·워크플로우·CLAUDE.md)을 도출한다. 사용자가 "세션 회고", "개밥먹기", "session retrospect", "지난 세션 분석", "이 프로젝트 세션에서 개선점"을 요청할 때 사용. 분석 대상 세션을 구동한 프로젝트 루트에서 실행하는 것을 전제로 한다.
---

# session-retrospect

wimcc(What's in My Claude Code)의 read-only 데이터로 최근 세션을 회고하고,
**이 프로젝트**의 개선 제안을 현재 컨텍스트에 바로 출력하는 워크플로우.

## 분업 원칙 (반드시 지킬 것)

- **측정은 wimcc, 판별은 LLM.** wimcc가 주는 것은 결정론적 카운트·증거 연결·집계뿐이다.
  "재작업 루프인가", "낭비인가" 같은 판단은 이 스킬을 실행하는 LLM(너)의 몫이다.
- 모든 주장에 근거를 붙인다: event_id, 횟수, 타임스탬프. 세션 1건 분석이면
  "표본 1"임을 명시하고 일반화하지 않는다.
- wimcc에는 어떤 쓰기도 하지 않는다 (read-only Pull API / MCP만 사용).

## Step 0 — 핸드셰이크

1. `curl -s http://127.0.0.1:7878/v1/health` — 실패하면 **여기서 멈추고** 안내:
   "wimcc가 실행 중이 아닙니다. `wimcc serve`로 기동하거나
   https://github.com/bahamoth/whats-in-my-cc 에서 설치하세요."
   transcript JSONL을 직접 파싱하는 fallback은 **금지** (그림자 구현 금지 원칙).
2. 아무 `/v1/*` 응답의 `meta.schema_version`을 확인한다. `0.5` 미만이면 wimcc
   업데이트를 안내하고 멈춘다. (`401`이면 `--auth on` 환경 — token 위치는
   macOS `~/Library/Application Support/wimcc/token`.)

## Step 1 — 세션 식별

현재 작업 디렉토리(프로젝트 루트)로 세션을 찾는다:

- MCP가 연결돼 있으면: `whats_in_my_cc.search_sessions` 도구에
  `{"project": "<현재 프로젝트 루트 절대경로>"}`.
- HTTP fallback: `GET /v1/sessions?project=<절대경로>` (URL 인코딩).

최신 세션이 기본 대상. 후보가 여럿이고 어느 것인지 불분명하면 사용자에게
목록(시각·이벤트 수)을 보여주고 고르게 한다.

## Step 2 — 결정론 데이터 수집

선택한 `session_id`에 대해:

1. **턴 집계** — `whats_in_my_cc.get_session_turns` (또는
   `GET /v1/sessions/:id/turns`): 턴별 tool histogram·편집 파일·user_message
   발췌, 그리고 `file_churn`(파일별 턴 수·편집 수).
2. **메트릭** — `GET /v1/sessions/:id/metrics`: tool 실패·중단·턴 시간 등.
3. **Signal** — `GET /v1/sessions/:id/signals`: detector가 발화한 Signal과
   evidence_refs.
4. 필요 시 특정 구간 원문 — `GET /v1/sessions/:id/events?kind=user_message`
   (kind는 CSV 가능: `user_message,tool_call`).

상세 절차와 판별 가이드: [references/workflow.md](references/workflow.md).

## Step 3 — LLM 판별

턴 집계를 읽고 다음 패턴을 *판단*한다 (wimcc는 판단하지 않는다):

- **재작업 루프 후보**: 짧은 user_message가 연속되고 같은 파일이 직전 턴에
  이어 재편집되는 구간 (`file_churn.turn_count` 높은 파일 + 해당 턴들의
  user_message 발췌를 읽고 교정인지 정상 반복인지 판별).
- **중복 작업 후보**: 두 파일이 턴마다 함께 편집됨 (churn 수치가 락스텝).
- **도구 미스매치**: 한 턴에 같은 도구가 비정상적으로 반복 (histogram).
- Signal의 tool_failure / context_bloat — `is_sidechain: true`인 context_bloat는
  보통 권장 위임 패턴이므로 문제로 단정하지 말 것.

## Step 4 — 산출

1. **개선 제안서를 현재 컨텍스트에 출력**: 항목마다 (근거: 관측 수치) →
   (제안: CLAUDE.md 한 줄 / 스킬 수정 / 워크플로우 변경). 사용자 승인을 받은
   항목만 그 자리에서 반영한다.
2. (선택) 사용자가 원하면 회고 문서를 `<프로젝트>/dogfood/<YYYY-MM-DD>-retrospect.md`
   형식으로 저장.
3. (선택) 분석 중 wimcc 자체의 마찰(없는 집계, 이상한 응답)을 발견하면
   wimcc 프로젝트에 전달할 피드백으로 별도 정리.
