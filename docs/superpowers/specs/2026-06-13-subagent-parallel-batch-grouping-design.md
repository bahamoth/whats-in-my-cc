# 서브에이전트 병렬 배치 그룹핑 — 스트리밍 생애주기 설계

- 날짜: 2026-06-13
- 상태: 설계 승인 대기(브레인스토밍 산출물)
- 범위: replay 스트림(webui)에서 병렬 디스패치된 서브에이전트를 "배치" 단위로 묶고, 스트리밍 중 점진적으로 구성하며, 각 에이전트의 결론과 배치 종합 결과를 표면화한다.

## 1. 문제

현재 replay 스트림(`webui/src/components/replay/stream/streamModel.ts`)은 sidechain(서브에이전트) 이벤트를 버퍼에 모으다가 **`agent_id`가 바뀔 때마다 그룹을 끊는다**(`closeGroup` on agent change, contiguity fallback). 이 규칙은 **병렬 실행에서 깨진다**.

근거(실측, 세션 `fb6b8e3a-2289-4214-884c-0c721a3e3cf5`, **표본 1 세션** — 일반화 주의):
- main이 한 assistant 메시지(`msg_01WPxhd…`, 같은 `turn_id`)에서 `Agent` 도구를 **5개** 디스패치 → 서브에이전트 5개 생성(`aaa02fd5`, `a40414dd`, `a985d5d0`, `a8dea898`, `a87b5264`).
- 디스패치 ↔ 서브에이전트는 **1:1**(`attachment_meta(subagent_meta).tool_use_id` ↔ 그 `tool_call`을 조인해 전수 확인; 한 tool_use_id당 distinct agent = 1).
- 5개가 **시간 겹침**으로 병렬 실행(예: `aaa02fd5` 12:33:27–12:36:00, `a40414dd` 12:33:36–12:34:58). 겹침 구간에서 이벤트가 **타임스탬프 순으로 촘촘히 교차**.
- 결과: agent 경계마다 끊는 그룹핑이 한 에이전트를 **여러 조각으로 쪼개고**, 인접 조각이 다른 에이전트라 "한 묶음에 두 에이전트가 섞여" 보이거나 "한 에이전트가 여러 그룹으로 갈라져" 보인다.

전 DB 검증: `Agent` 디스패치 **132건 전부 main 체인(sidechain=0)**, 서브에이전트가 또 디스패치한 경우 **0건** → 현재 데이터에 **중첩 없음**(모두 main 직속 형제). 단 코드가 이를 하드 가정하므로 중첩 발생 시 평면화됨(아래 degrade 항목).

## 2. 개념 모델 (확정)

```
main agent  (유일한 디스패처)
  └─ 한 턴(=한 assistant 메시지)에서 Agent 호출 N개 ── 각 1:1 ──▶ 서브에이전트 N개  → 병렬 실행
```

- **묶음 단위 = 병렬 배치**(= 한 디스패치 턴의 형제 집합), 개별 에이전트가 아니다.
- 동시성은 **에이전트 사이에만** 존재하고 **한 에이전트 안은 직렬**이다. 이 사실이 설계의 핵심 — 배치 컨테이너가 동시성을 흡수하고, 각 에이전트 자식은 직렬 sub-stream으로 깔끔하다.

## 3. 설계

### 3.1 스트림 모델 변경 (평탄 append → 라우팅)

- 새 항목 타입 `batch-group`(컨테이너) 도입. 자식은 기존 `sidechain-group`(에이전트별) — 단 이제 **시간 교차와 무관하게 agent_id로 전역 수집**한다(de-interleave).
- **배치 멤버십**: 디스패치 턴(같은 `message_id`/`turn_id`에서 발사된 `Agent` tool_call들)이 한 배치. 각 tool_call의 `tool_use_id`로 자식 에이전트를 연결(`subagent_meta.tool_use_id` ↔ tool_call; agent_id ↔ tool_use_id).
- `buildStreamModel`은 sidechain 이벤트를 **열린 배치의 해당 agent 자식 블록으로 라우팅**(agent_id 키 맵 유지). "agent_id 바뀌면 끊기"를 대체.

### 3.2 스트리밍 생애주기 (점진적)

완료를 기다리지 않는다. (wimcc는 라이브 SSE/transcript-tail 스트림 — replay 전용 아님.)

1. **디스패치 시점**: 배치 경계(N·각 description)가 즉시 확정(같은 turn) → 배치 컨테이너를 `진행 중`으로 연다.
2. **스트리밍 중**: 도착하는 sidechain 이벤트를 agent_id로 자식에 append. 끝난 에이전트는 결론 표시, 도는 에이전트는 `⏳`.
3. **완료(모든 에이전트 종료 + main 재개)**: 배치 `완료`로 전환, 총 소요·종합 결과 채움.

가상화(`ConversationStream` virtualizer)는 이미 가변 높이를 다루므로 컨테이너 성장은 수용된다.

### 3.3 결론·종합 표면화

- **에이전트 결론**: 각 서브에이전트의 **마지막 `assistant_message`**(actor=assistant)를 결론으로 요약해 축약 줄에 표시. (실측: 모든 sidechain agent의 마지막 이벤트가 assistant_message.)
- **배치 종합 결과**: 모든 에이전트 반환 후 **main의 종합 메시지**(배치 이후 첫 main assistant_message)를 배치 outcome 줄로 끌어올림. 스트리밍 중엔 `진행 중`.

### 3.4 2단계 접기 구조 (확정)

접이 지점은 둘(배치, 서브에이전트):

- **L0 — 가장 접힘(배치 접힘)**: 한 줄/블록에 **배치 정체성 + 종합 결과 둘 다** 표시. 펼치지 않고도 "무슨 배치였고 결론이 뭔지" 파악. (종합 결과를 이 줄로 끌어올린다.)
- **L1 — 배치 펼침**: 각 **서브에이전트 요약 줄**(`description + 결론 + meta`).
- **L2 — 서브에이전트 펼침**: 그 에이전트 **상세**(내부 sub-stream: prompt·활동·메시지).

기본 상태 = **L0(접힘, 배치+종합)**. 스트리밍 중엔 L0의 종합 자리가 `진행 중`이다가 main 재개 시 채워지고, 진행 중일 때는 진행 파악을 위해 L1로 자동 펼침을 둘 수 있다(보조).

## 4. 컴포넌트

- `BatchGroup`(신규): 컨테이너 — 헤더(개수·진행 상태·총 소요·접기), 자식 `SubagentGroup` 목록, outcome(종합) 줄.
- `SubagentGroup`(기존, 수정): 축약 줄에 결론 추가, 진행/완료 상태 칩.
- `streamModel`(수정): `batch-group` 생성·라우팅.
- `duration`(재사용): heat·포맷.

각 단위는 단일 책임 + 명확한 인터페이스(입력 이벤트 → 항목 트리)로 독립 테스트 가능하게 유지.

## 5. 데이터 흐름·키

| 목적 | 키/출처 |
|---|---|
| 배치 멤버십 | `Agent` tool_call들의 동일 `message_id`/`turn_id` |
| 디스패치↔에이전트 연결 | `subagent_meta.tool_use_id` ↔ tool_call · `agent_id` |
| 자식 라우팅 | sidechain 이벤트의 `agent_id` |
| 에이전트 결론 | 그 agent의 마지막 `assistant_message` |
| 배치 종합 | 배치 종료 후 main의 첫 `assistant_message` |

모든 키는 events DTO에 이미 노출(`message_id`·`turn_id`·`agent_id`·`tool_use_id`·`is_sidechain`). 추가 백엔드 변경 불필요(검증: routes.rs DTO에 존재).

## 6. 에러·degrade 처리

- **pre-0023 ingest(agent_id 없음)**: agent_id 라우팅 불가 → 기존 contiguity 그룹핑으로 degrade(배치 미형성).
- **사이드카 미도착/지연**: agent_id↔tool_use_id 연결이 없으면 배치 귀속 불가 → 미귀속 sidechain 이벤트는 임시로 **배치 하단 "분류 중"** 버킷에 두고, 연결 도착 시 해당 자식으로 이동.
- **중첩 디스패치(미관측)**: 현재 데이터엔 없음. 발생 시 평면화(자식 안의 또 다른 배치는 1단계만) — 별도 후속.
- **단일 디스패치(N=1)**: 배치 컨테이너 생략하고 기존 단일 `SubagentGroup`으로(불필요한 래퍼 방지).

## 7. 트레이드오프

- **잃는 것**: 메인 흐름에서 **에이전트 간 정밀 교차 순서**(A의 3번째 도구가 B의 5·6번째 사이). 읽기용엔 거의 무의미하고, 정밀 타이밍은 별도 뷰(waterfall — 미구현, 토큰만 존재)의 몫.
- **얻는 것**: 시간순(채팅 불변식) 유지 + 에이전트별 일관 블록 + 결론·종합 즉시 파악.
- **비용**: 스트림 모델이 평탄 append → 라우팅으로 복잡도↑. 사이드카 라이브 타이밍 의존.

## 8. 테스트 (TDD red 우선)

- `buildStreamModel`: 교차 입력(겹친 두 agent 이벤트)이 **배치 1개 + 자식 2개(각 직렬)**로 묶이는가(고정 fixture).
- 점진성: 부분 입력(2 완료 + 3 진행)에서 진행 상태·결론 pending이 맞는가.
- 결론 추출: 마지막 assistant_message가 결론으로 잡히는가.
- 종합: 배치 후 main 메시지가 outcome으로 연결되는가.
- degrade: agent_id 없는 입력 → contiguity fallback / 미귀속 → "분류 중".
- 실 fixture는 `tests/fixtures/transcripts/real/` 동결 데이터 사용(real-data anchoring). 표본 1 세션 기반임을 명시.

## 9. 범위 밖 / 후속

- **우측 scaffold(command/skill) 모아보기**: 첫 UX 요청. 연속 동일-origin scaffold 묶기로, 본 배치 컨테이너의 접이식 패턴을 **재사용**할 수 있으나 별 슬라이스로 분리.
- **violet 뱃지(scaffold origin 색)**: 별건, 작업트리에 적용됨(미커밋) — 본 설계와 독립.
- **waterfall/lane 동시성 뷰**: 정밀 타이밍 분석용, 본 설계와 별개.

## 10. 열린 질문

1. ~~배치 기본 접힘/펼침 정책~~ → **확정(§3.4)**: 기본 L0(접힘, 배치+종합), 진행 중 보조 자동 펼침.
2. ~~종합 결과 끌어올림 vs 링크~~ → **확정(§3.4)**: L0 줄로 끌어올림(원본 main 메시지는 스트림에 그대로 둠).
3. "분류 중" 버킷 노출 방식(항상 보임 vs 연결되면 사라짐). (미해결)
