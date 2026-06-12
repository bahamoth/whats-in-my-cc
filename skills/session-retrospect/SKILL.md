---
name: session-retrospect
description: wimcc로 관측된 Claude Code 세션을 회고 분석해 그 프로젝트의 개선 제안(프롬프트·스킬·워크플로우·CLAUDE.md)을 도출한다. 사용자가 "세션 회고", "개밥먹기", "session retrospect", "지난 세션 분석", "이 프로젝트 세션에서 개선점"을 요청할 때 사용. 분석 대상 세션을 구동한 프로젝트 루트에서 실행하는 것을 전제로 한다.
---

# session-retrospect

wimcc(What's in My Claude Code)의 read-only 데이터로 최근 세션을 회고하고,
**이 프로젝트**의 개선 제안을 현재 컨텍스트에 바로 출력하는 워크플로우.

제안은 **반증 가능한 예측**(개선될 지표·악화 가능 지표)과 함께 원장
(`dogfood/`)에 남기고, 다음 회고가 그 예측을 전후 비교로 검증한다 —
제안→채택→재측정이 이어져야 루프가 닫힌다.

## 분업 원칙 (반드시 지킬 것)

- **측정은 wimcc, 판별은 LLM.** wimcc가 주는 것은 결정론적 카운트·증거 연결·집계뿐이다.
  "재작업 루프인가", "낭비인가" 같은 판단은 이 스킬을 실행하는 LLM(너)의 몫이다.
- 모든 주장에 근거를 붙인다: event_id, 횟수, 타임스탬프. 세션 1건 분석이면
  "표본 1"임을 명시하고 일반화하지 않는다.
- wimcc에는 어떤 쓰기도 하지 않는다 (read-only Pull API / MCP만 사용).
- **기억은 repo에.** 회고의 기억(제안 원장·채택 여부·전후 비교 결과)은 git이
  추적하는 `dogfood/` 문서가 담당한다 — wimcc는 측정만, 원장은 repo가, 판단은 LLM이.

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

## Step 1.5 — 이전 회고 원장 로드

1. `ls <프로젝트 루트>/dogfood/`에서 `*retrospect*.md`를 찾아 최신 1~2개를
   읽는다. 없으면 첫 회고 — Step 2로 진행.
2. 이전 제안 표의 각 항목(ID `R-YYYYMMDD-n`)에 대해 **채택 여부**를 판정한다:
   제안이 가리키는 파일의 현재 상태 확인 또는
   `git log --oneline -S"<제안 핵심 문자열>" -- <파일>`. 판정 근거(커밋 해시)를
   남긴다.
3. 채택된 제안이 있으면 Step 5(전후 비교)를 수행한다 — 이번 회고 보고서에
   "이전 예측 검증" 절이 들어가야 한다.

## Step 2 — 결정론 데이터 수집

선택한 `session_id`에 대해:

1. **턴 집계** — `whats_in_my_cc.get_session_turns` (또는
   `GET /v1/sessions/:id/turns`): 턴별 tool histogram·편집 파일·user_message
   발췌, 그리고 `file_churn`(파일별 턴 수·편집 수).
2. **메트릭** — `GET /v1/sessions/:id/metrics`: tool 실패·중단·턴 시간 등.
3. **Signal** — `GET /v1/sessions/:id/signals`: detector가 발화한 Signal과
   evidence_refs.
4. **Fingerprint** — `GET /v1/sessions/:id/fingerprint`: 이 세션이 어떤
   모델·CC 버전·branch·instruction(CLAUDE.md 해시) 아래에서 돌았는가.
   (hook collector 미설치 또는 과거 세션은 `claude_md`가 빈다 — 결측은
   결측으로 보고하고 instruction 코호트 분할에 쓰지 않는다.)
5. 필요 시 특정 구간 원문 — `GET /v1/sessions/:id/events?kind=user_message`
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
- Signal 해석 시 `/v1/detectors` manifest의 `metric_class`를 참고한다:
  `process`(행동 형태 — 회피 가능) 지표의 개선 주장에는 `outcome`(최종 상태
  결부) 지표 동반 확인이 필요하다.

## Step 4 — 산출 (제안 = 반증 가능한 예측)

1. **개선 제안서를 현재 컨텍스트에 출력**. 항목마다:
   - **ID**: `R-<YYYYMMDD>-<n>` — 원장 추적 키. 다음 회고가 이 ID로 채택
     여부·효과를 판정한다.
   - **근거**: 관측 수치·event_id·턴 시각.
   - **제안**: CLAUDE.md 한 줄 / 스킬 수정 / 워크플로우 변경 (적용 가능한 것은
     diff 형태로).
   - **예측**: 채택 시 개선될 지표(이름·방향)와 악화될 수 있는 지표(반작용).
     예측 없는 제안은 다음 회고가 검증할 수 없다 — 반드시 적는다.
   사용자 승인을 받은 항목만 그 자리에서 반영한다.
2. 회고 문서를 `<프로젝트>/dogfood/<YYYY-MM-DD>-retrospect.md` 형식으로
   **저장한다** (기본 동작 — 원장이 없으면 다음 회고가 비교할 대상이 없다.
   사용자가 원치 않으면 생략).
3. (선택) 분석 중 wimcc 자체의 마찰(없는 집계, 이상한 응답)을 발견하면
   wimcc 프로젝트에 전달할 피드백으로 별도 정리.

## Step 5 — 전후 비교 (이전 회고의 채택 제안 검증)

Step 1.5에서 채택된 제안을 발견했을 때만 수행. 상세 절차는
references/workflow.md의 "전후 비교 절차" 절.

요약: `whats_in_my_cc.get_project_metrics {"project": "<루트>"}` (HTTP
fallback `GET /v1/metrics?project=`)로 세션 series를 받아, 채택 커밋 시각
또는 `fingerprint.instruction_sha256` 변화로 전/후 코호트를 나누고
**예측했던 지표만** 비교한다. 보고 규칙:

- 전/후 표본 수를 명시한다. 한쪽이 3 미만이면 "참고 수준"으로 강등.
- 혼재 요인 점검: 전/후 코호트의 `models`·`cc_versions`가 다르면 명시하고
  단정하지 않는다.
- process 지표만 개선되고 outcome 지표(verification 계열)가 정체·악화하면
  지표 게임 가능성을 함께 보고한다.
- 판정(검증됨/반증됨/판정불가)을 이번 회고 문서의 "이전 예측 검증" 절에
  기록한다 — 반증된 제안은 되돌리는 제안을 새 ID로 낸다.
