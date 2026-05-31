# Episode 분류 — 문제제기 (defer: 머지 후 별도 세션에서 해결)

- 날짜: 2026-05-31
- 발견 맥락: PR #25(facet correlation) 브라우저 검증 중, 메시지 뷰 카드의 phase 배지를 살펴보다 사용자가 제기.
- 상태: **기록만.** 이 PR에서 고치지 않음. 머지 후 별도 세션에서 다룬다.
- 관련 기존 트랙: `docs/superpowers/plans/2026-05-30-episode-classifier-drift-fix.md`
- 관련 코드: `src/insight/episode/classifier.rs`(분류), `src/insight/episode/types.rs`(Phase enum), `src/graph/build.rs` `rebuild_session`(episode 영속), `webui/src/routes/SessionDetailPage.tsx`(`phaseByEventId`/`phaseOf` — 프론트 시간-창 매칭).

## 사용자의 문제제기 (원문 의도)

1. **이 에피소드 구분이 정당한가 / 인사이트를 주는가?**
   - 분류를 보고 실제 인사이트를 얻을 수 있는가?
   - `drift` 반복 = "헤매고 있다" 신호로 읽어도 되나?
   - `exploration` 반복 = "자료조사가 너무 길다"는 의미인가?
2. **에피소드 획득은 제대로 되고 있나? 오류는 없나?**
3. **여러 동작을 하나의 에피소드로 묶는 이 기준이 맞나?**
   - 사용자 의도: 채팅과 채팅 사이의 추론·tool_call 등을 **간결하게** 보여줘야 추적이 가능하다.
   - 우려: 여러 종류의 행위를 잘못 묶으면 오히려 문제 파악이 어려워진다.

## 현재 동작 (코드 기준, 검증됨)

- **분류는 백엔드.** `classify_session`이 rebuild 때 전체 이벤트를 순회하며 각 이벤트를 한 phase에 넣고, 같은 phase 연속 구간을 한 에피소드로 emit → `episode` 테이블 저장.
- **프론트는 분류하지 않음.** `phaseOf`는 `eps.find(e => e.started_at <= t && t <= e.ended_at)?.phase` — 이벤트 시각이 들어가는 **첫 매칭 에피소드**의 phase를 배지로 붙일 뿐.
- Phase 7종: `intake`·`exploration`·`diagnosis`·`action`·`verification`·`repair`·`drift`.
- phase 드라이버: user_message→intake / 변경툴(Edit·Write·MultiEdit·Bash)→action(실패후 repair) / read-only툴(Read·Grep·Glob·LS·WebFetch·WebSearch)→exploration(에러뒤 diagnosis) / 검증트리거→verification / read-only **8연속**(`DRIFT_THRESHOLD=8`)→drift.
- **그 외 모든 이벤트**(MCP 툴콜·ToolSearch·attachment_meta·session_state·thinking·assistant_message·hook·정상 tool_result)는 **"현재 phase 상속"** — 경계를 만들지 않음.

## 진단된 문제

### A. 정확성 버그 (지금 배지가 틀림)
1. **에피소드 누적**: `rebuild_session`은 graph는 매 rebuild `delete`-후-`insert`하지만 **`episode` 테이블은 안 지운다.** 그래서 rebuild마다 "마지막 열린 에피소드"(예: 그 시점 current_phase=action → `[action 시작 … 그때의 마지막 이벤트]`)가 새로 쌓여, **넓은 stale 에피소드들이 겹쳐 누적**된다.
   - 증거: 세션 `2c5d9a5a…`에서 한 시각을 덮는 `action` 에피소드가 **22개** 겹침(전부 시작 17:22:27, 끝만 제각각).
2. **프론트의 겹침 해소가 틀림**: `phaseOf`가 "첫 매칭"을 고르는데, 겹치면 **가장 일찍 시작한(=가장 넓은, stale) 것**이 먼저 잡힌다.
   - 재현: 노드 `nd_e38cd83814185afe4186368f` = `Read` 툴콜(17:32:44). 이 시각을 덮는 에피소드 = `action` 1개([17:22:27→17:32:52], stale·넓음) + `exploration` 3개([17:32:44→…], 정확·좁음). 백엔드는 옳게 exploration으로 분류했지만, action이 먼저 잡혀 **화면엔 action**으로 뜬다(= Read인데 action 배지).
   - 또 다른 예: `nd_0c9d35818322ee5d891ee856`(MCP `browser_batch`) 구간 — Bash·MCP·ToolSearch·attachment_meta·session_state·thinking·메시지가 전부 한 `action` 덩어리로 묶임(비드라이버 상속 때문).

### B. 개념적 약함 (버그를 고쳐도 남는 한계)
1. **휴리스틱이 얕다**: `drift = read-only 8연속`은 *헤맴*일 수도 *정상적인 꼼꼼한 조사*일 수도 있다. 분류기는 그 조회가 **생산적/수렴적이었는지 모른다.** → "drift 반복=헤맴", "exploration 반복=조사 과다"는 **오탐 많은 약한 힌트**이지 신뢰 신호가 아니다.
2. **confidence가 고정 상수**(action 0.95·drift 0.6 등) — 데이터로 측정한 값이 아님. "evidence-linked" 원칙에 비춰 신뢰 근거가 약함.
3. **비드라이버 상속 → 거친 블롭**: MCP·ToolSearch·attachment·session_state·thinking 등이 전부 주변 phase를 상속하니, 한 에피소드가 수십 개 이질적 이벤트를 한 라벨로 빨아들인다. 라벨은 드라이버(예: Bash)로만 대표되어 나머지가 가려진다.

### C. 추적 목표와의 입자 불일치
- 사용자 목표(채팅 사이 행위를 간결히 추적)에는 **거친 phase 블롭이 1차 렌즈로 부적합.** 행동/턴 단위의 간결한 트레이스(각 tool_call+결과+주변 추론을 접어서)가 더 맞고, phase는 (버그 수정 후) **보조 개요**여야 한다.

## 다음 세션을 위한 열린 질문
- 에피소드를 **유지할지/재설계할지/보조로 강등할지** 자체를 결정(brainstorming 권장).
- 최소 수정: (a) `rebuild_session`에서 세션 episode를 rebuild 전 `delete`(graph처럼) → 누적 제거, (b) 프론트 `phaseOf`의 겹침 해소를 "가장 좁은/가장 늦게 시작한 매칭" 또는 결정적 규칙으로 → stale 우선 제거.
- 개념 개선: drift/exploration을 *길이*가 아니라 *수렴/생산성* 신호로 측정할 수 있는가? confidence를 데이터 기반으로?
- 입자: 추적 1차 렌즈는 턴/행동 단위로, episode는 개요로 분리?

## 결정
**이 PR(#25, facet)에서는 손대지 않는다.** 머지 후 별도 세션에서 위 A(버그) → B/C(설계) 순으로 다룬다.
