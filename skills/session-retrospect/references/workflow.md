# session-retrospect — 상세 워크플로우

SKILL.md의 Step 2–5를 수행할 때의 세부 가이드. 이 워크플로우 자체가
2026-06-12 개밥먹기(세션 `191eddf3`, lhh-liveops)에서 수작업으로 검증된
절차의 정식화이며, 가설 원장·전후 비교는 같은 날 loop-foundations 라운드에서
추가됐다.

## 데이터 표면 치트시트

| 목적 | MCP 도구 | HTTP |
|------|---------|------|
| 프로젝트의 세션 찾기 | `search_sessions {project}` | `GET /v1/sessions?project=` |
| 턴 집계 + 파일 churn | `get_session_turns {session_id}` | `GET /v1/sessions/:id/turns` |
| 세션 메트릭 | `get_session_metrics {session_id}` | `GET /v1/sessions/:id/metrics` |
| 세션 fingerprint (모델·CC버전·branch·cwd·entrypoint) | `get_session_fingerprint {session_id}` | `GET /v1/sessions/:id/fingerprint` |
| 세션 횡단 series (전후 비교) | `get_project_metrics {project?, from?, to?, limit?}` | `GET /v1/metrics?project=&from=&to=&limit=` |
| Signal 목록 | `get_session_signals {session_id}` | `GET /v1/sessions/:id/signals` |
| detector manifest (metric_class 포함) | `list_detectors` | `GET /v1/detectors` |
| kind 필터 이벤트 | — | `GET /v1/sessions/:id/events?kind=a,b&limit=` |
| 특정 tool_use 상관 조회 | — | `GET /v1/sessions/:id/events?tool_use_id=` |

events 커서는 `data.prev_cursor` / `data.next_cursor` (URL 인코딩 필요).
`next_cursor: null` = 해당 스트림의 live tip. 미지원 쿼리 파라미터는 400으로
거부되므로 silent-drop을 의심할 필요 없다. `/v1/metrics`의 `matched_count`가
`session_count`보다 크면 limit 절단이 일어난 것이다 — 절단을 숨기지 말 것.

## 판별 휴리스틱 (LLM 몫 — wimcc 데이터에 없는 이유)

이 절의 패턴들은 의미 해석이라 결정론 규칙으로 환원되지 않는다. wimcc가
"교정성 메시지" 같은 lexical 휴리스틱을 탑재하지 않는 것은 의도된 설계다
(가짜 결정론 금지). 그래서 판별은 이 스킬에서 한다:

1. **재작업 루프**: `turns[]`에서 `tool_call_total`이 작고 간격이 짧은
   user_message 연속 구간을 찾고, 그 턴들의 `files_edited`가 직전 턴과
   겹치는지 본다. 겹치면 해당 user_message 발췌를 읽고 *교정*(불만/정정)인지
   *정상 반복*(새 요구)인지 판별한다. 교정 연속 ≥2면 개선 제안 후보:
   "구현 전 무엇을 확인했어야 했나"를 역산한다.
2. **중복 편집 스트림**: `file_churn`에서 edit_count가 비슷하게 높은 파일
   쌍이 있으면 같은 턴들에서 편집됐는지 확인(`turns[].files_edited`).
   락스텝이면 "한쪽을 생성물로 전환(빌드 스텝)" 제안 후보.
3. **도구 미스매치**: 한 턴의 histogram에서 같은 도구 15회 이상 반복이면
   그 턴의 목적(user_message)과 도구가 맞는지 본다 (예: 애니메이션 검증에
   정적 스크린샷 반복 → gif_creator 제안).
4. **Signal 해석**: `tool_failure`는 후속 이벤트(`events?tool_use_id=`)로
   자가 회복 여부를 확인하고, 회복됐으면 심각도를 낮춰 보고한다.
   `context_bloat`의 `facts.tool_result.is_sidechain == true`는 Agent 위임
   패턴일 가능성이 높다 — 문제로 단정하지 않는다.
5. **최종 상태 불일치** (구 `final_state_mismatch` detector — 2026-07-03
   L1에서 제거, 판별이 여기로 이관됨): 마지막 턴의 user_message가 목표
   (수정/구현/고침 요구)를 표현했고, `verification-runs`의 마지막 결과가
   failed인데, 마지막 assistant 메시지가 완료를 선언했는지를 *언어 무관하게*
   읽고 판단한다. 재료는 결정론 측정값 그대로다 — `get_session_turns`의
   user_message 발췌 + `GET /v1/sessions/:id/verification-runs` +
   SessionMetrics의 `verification_failed`. lexical 마커(영어 동사 목록)로
   환원되지 않아 detector가 아니라 LLM 몫이 된 대표 사례.

## 전후 비교 절차 (Step 5)

이전 회고의 **채택된** 제안 `R-YYYYMMDD-n`에 대해, 예측했던 지표가 실제로
움직였는지 판정한다.

1. **코호트 분할 기준** (우선순위 순):
   1. 채택 커밋 시각 — `git log -1 --format=%cI <commit>` vs 각 세션의
      `first_observed_at`. CLAUDE.md/instruction 변경 제안도 동일하게 그
      변경 커밋 시각이 경계다. (fingerprint의 instruction 스냅샷 필드
      `instruction_sha256`/`claude_md`는 2026-06-19 hook collector와 함께
      제거됨 — git 이력이 이 역할을 대신한다.)
   2. 사용자 수동 지정.
2. **수집**: `get_project_metrics {"project": "<루트>", "limit": 50}`.
   필요하면 `from`/`to`(RFC3339)로 창을 좁힌다.
3. **비교**: 예측에 적힌 지표만 본다(사후 지표 쇼핑 금지 — 그게 원장의 존재
   이유다). count는 세션 길이에 좌우되므로 비교 전에 분모를 명시해 직접
   나눈다(F1: 비율은 소비자가 계산). 예: `tool_failure_count /
   tool_call_total`, `turn_duration_ms_total / turn_duration_count`.
4. **보고 형식**:

   ```
   ## 이전 예측 검증 (R-YYYYMMDD-n)
   | 제안 ID | 예측 | 전 (n=) | 후 (n=) | 판정 |
   ```

   판정은 검증됨 / 반증됨 / 판정불가(표본 부족·혼재 요인) 셋 중 하나.
5. **단정 금지 규칙**: 전/후 한쪽 표본이 3 미만이면 "참고 수준". 전/후
   코호트의 `models`·`cc_versions`가 다르면 혼재 요인으로 명시. 작업 난이도는
   관측 불가 — "같은 프로젝트의 인접 기간"이라는 약한 통제만 가능함을 적는다.
   반증된 제안은 되돌리는 제안을 새 ID로 낸다.

## Goodhart 주의 (지표가 목표가 될 때)

- `/v1/detectors` manifest의 `metric_class`를 본다: `process`(행동 형태 —
  지표를 피하는 행동 변화로 게임 가능) vs `outcome`(최종 상태 결부 — 게임
  난도 높음).
- process 지표만 개선되고 outcome 지표(verification 계열)가 정체·악화하면
  "증거 회피" 가능성을 보고한다. (outcome 축은 verification_run 측정면이
  담당한다 — 유일한 outcome-class detector였던 final_state_mismatch는
  2026-07-03 L1에서 제거되고 판별이 위 휴리스틱 5로 이관됐다.)
- 제안을 낼 때 자문: 이 제안은 *신호를 없애는가*(예: 재읽기 신호를 피하려
  통파일 읽기 → context_bloat로 전이), *원인을 없애는가*?

## 판별→fixture 승격 (detector 후보의 공식 졸업 관문)

- 회고에서 같은 구조 패턴이 **서로 다른 세션 2개 이상**에서 확정되면, 해당
  사례의 실 payload를 wimcc 리포의 `tests/fixtures/**/real/`에 동결하고
  invariant 테스트로 잠근다 (Real-data anchoring의 확장).
- 이것이 보류 중 detector 후보(`re_edit_churn`, `duplicate_edit_stream`)의
  공식 졸업 경로다: **표본 축적은 annotation이 아니라 fixture로 한다.**
  wimcc에 라벨/판정을 쓰는 API는 만들지 않는다(no-annotation 원칙 유지).

## 알려진 한계

- 세션 1건 = 표본 1. 같은 패턴이 다른 세션에서도 보이는지 확인 전에는
  프로젝트 차원의 규칙 변경을 강하게 권하지 않는다.
- turns 집계는 conversation kind(user_message/assistant/tool_call/tool_result)
  기준이다. 텔레메트리 전용 세션이면 비어 있을 수 있다.
- 비코드 프로젝트의 검증 활동(브라우저 smoke)은 verification-runs에 잡히지
  않는다 (wimcc 알려진 사각지대, 2026-06-12 기준).
- fingerprint에 instruction(CLAUDE.md) 스냅샷은 없다 — `claude_md`/
  `instruction_sha256` 필드는 hook collector 폐지와 함께 2026-06-19 제거됐다.
  transcript에는 CLAUDE.md가 기록되지 않으므로(2026-06-12 실측: 4개 프로젝트
  12 transcript 음성) 세션별 instruction 소급은 불가하고, 커밋된 프로젝트
  CLAUDE.md에 한해 세션 시각 × git history로 근사만 가능하다.
