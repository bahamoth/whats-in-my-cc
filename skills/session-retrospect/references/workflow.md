# session-retrospect — 상세 워크플로우

SKILL.md의 Step 2–4를 수행할 때의 세부 가이드. 이 워크플로우 자체가
2026-06-12 개밥먹기(세션 `191eddf3`, lhh-liveops)에서 수작업으로 검증된
절차의 정식화다.

## 데이터 표면 치트시트

| 목적 | MCP 도구 | HTTP |
|------|---------|------|
| 프로젝트의 세션 찾기 | `search_sessions {project}` | `GET /v1/sessions?project=` |
| 턴 집계 + 파일 churn | `get_session_turns {session_id}` | `GET /v1/sessions/:id/turns` |
| 세션 메트릭 | — | `GET /v1/sessions/:id/metrics` |
| Signal 목록 | — | `GET /v1/sessions/:id/signals` |
| kind 필터 이벤트 | — | `GET /v1/sessions/:id/events?kind=a,b&limit=` |
| 특정 tool_use 상관 조회 | — | `GET /v1/sessions/:id/events?tool_use_id=` |

events 커서는 `data.prev_cursor` / `data.next_cursor` (URL 인코딩 필요).
`next_cursor: null` = 해당 스트림의 live tip. 미지원 쿼리 파라미터는 400으로
거부되므로 silent-drop을 의심할 필요 없다.

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

## 제안서 형식

```
## <프로젝트명> 세션 회고 (YYYY-MM-DD, 세션 <id 앞 8자>, 표본 1)

| # | 제안 | 근거(관측) | 기대 효과 |
|---|------|-----------|----------|
```

- 근거 열에는 수치·event_id·턴 시각만. 해석은 본문 문단에.
- 적용 가능한 제안(CLAUDE.md 한 줄, 스킬 규칙 추가)은 diff 형태로 제시하고
  승인 후 반영.

## 알려진 한계

- 세션 1건 = 표본 1. 같은 패턴이 다른 세션에서도 보이는지 확인 전에는
  프로젝트 차원의 규칙 변경을 강하게 권하지 않는다.
- turns 집계는 conversation kind(user_message/assistant/tool_call/tool_result)
  기준이다. 텔레메트리 전용 세션이면 비어 있을 수 있다.
- 비코드 프로젝트의 검증 활동(브라우저 smoke)은 verification-runs에 잡히지
  않는다 (wimcc 알려진 사각지대, 2026-06-12 기준).
