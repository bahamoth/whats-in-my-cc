# 개밥먹기 회고 — wimcc 개선 제안 (2026-06-12)

> **출처**: 세션 `191eddf3-7009-4ca1-9f26-32dcd11b9a1d`(lhh-liveops, 2.4시간,
> 6,533 이벤트)를 wimcc Pull API로 분석해 프로젝트 개선 제안을 도출한 경험.
> 이 문서는 그 과정에서 드러난 wimcc 자체의 개선 제안과, 향후 이 과정을
> skill/MCP로 정례화하는 방안을 담는다.
>
> **용어**: 산출물은 **Signal** 하나로 통일. Signal을 만드는 규칙은
> **Signal 규칙(detector)**. evidence_refs·L1은 Signal의 내부 필드/등급 표기.

## 1. 핵심 결론 — 입증된 분업 모델

이번 분석에서 가치의 대부분은 Signal이 아니라 **event-first 리플레이 기반**
(턴 분절, tool_call↔result 페어링, sidechain 구분)에서 나왔다. Signal 3건
(context_bloat 1, tool_failure 2)은 세션의 지배적 비용(21분 재작업 루프,
44% 중복 편집)을 모두 놓쳤다.

입증된 모델: **wimcc는 결정론적 측정·증거 연결·집계를 제공하고, 의미 판별은
LLM이 온디맨드로 한다.** 이는 untagged-bash 루프·unknown-verification 루프와
같은 계보이며, Signal에 severity/confidence를 저장하지 않는 기존 스키마 철학과
일치한다. "교정성 사용자 메시지" 같은 의미 판별을 lexical 휴리스틱으로 흉내 내는
가짜 결정론은 만들지 않는다.

**투자 우선순위: Signal 규칙 추가보다 집계 표면 확장(§3)이 먼저.**

## 2. Signal 규칙(detector) 제안 — 구조 신호만

| 후보 | 정의 | 이번 세션 관측 |
|------|------|---------------|
| `re_edit_churn` | 같은 파일이 N개 이상 사용자 턴에 걸쳐 재편집 | standalone 9턴, index 8턴 |
| `duplicate_edit_stream` | 두 파일이 락스텝으로 유사 편집 스트림 수신 | 전체 편집의 44% 중복 |

- ~~`rework_loop`~~ — **철회.** "교정" 여부는 의미 해석이라 결정론 규칙으로
  환원 불가. 판별은 wimcc 데이터를 읽는 LLM의 몫.
- **단서**: 표본 1 세션. 위 두 후보가 다른 세션에서도 발화하는 패턴인지 확인 후
  구현 착수 (CLAUDE.md "표본 1건으로 일반화하지 않는다" 원칙).

## 3. API/MCP — LLM의 판별 비용을 낮추는 집계 표면

이번 분석에서 커스텀 Python 스크립트로 직접 만들어야 했던 것들이 곧 요구 명세다.
이것들이 API/MCP 도구로 있으면 세션 분석이 스크립트 없이 툴콜 몇 번으로 끝난다.

1. **`kind` 필터**: `GET /v1/sessions/:id/events?kind=tool_call`이 현재 **조용히
   무시**된다(axum Query가 미정의 파라미터 묵살 — hook_event가 반환됨). kind 필터
   추가 + 미지원 파라미터 400 거부 검토.
2. **턴 단위 집계 endpoint**: 사용자 턴별 tool histogram · 파일별 churn(턴 수,
   편집 수) · turn 경계가 붙은 user_message 목록.
3. **프로젝트→세션 매핑**: 프로젝트 루트 경로로 세션을 찾는 필터
   (`GET /v1/sessions?project=<path>` 류). §5의 skill 워크플로우의 전제 조건 —
   "방금 이 프로젝트에서 돌린 세션"을 wimcc가 찾아줄 수 있어야 한다.
4. **envelope 불일치**: `meta.next_cursor`는 항상 null인데 실제 커서는
   `data.next_cursor`에 있다. 한쪽으로 통일.

## 4. 기존 Signal 품질

1. **context_bloat의 tool_name 미해석**: summary가 `from ""`로 출력됨 —
   tool_result 이벤트의 tool_name이 비어 있으면 페어링된 tool_call에서 해석할 것
   (이번 건의 실제 도구는 sidechain의 Read).
2. **context_bloat의 sidechain 인지**: Agent 위임으로 큰 문서를 sidechain에서
   읽는 것은 *권장* 패턴인데 현재 동일하게 발화 — facts에 `is_sidechain` 명시
   또는 발화 강등 검토.
3. **verification guard의 사각지대**: 이 세션 verification_total=0. 비코드
   프로젝트의 검증 활동(claude-in-chrome 브라우저 smoke, 64콜)이 전혀 분류되지
   않는다. 브라우저 툴콜을 검증 활동 카테고리로 다룰지 검토.

## 5. 정례화 방안 — 세션 회고 skill + wimcc MCP

**목표 워크플로우**: 사용자가 *분석 대상 세션을 구동한 프로젝트 루트*에서 회고를
가동하면, 개선 제안이 그 프로젝트의 LLM 컨텍스트로 바로 전달된다.

```
[프로젝트 X 루트에서]  /session-retrospect
        │
        ▼
  ① skill이 wimcc MCP로 이 프로젝트의 최근 세션 식별   ← §3-3 필요
  ② 결정론 데이터 수집: metrics · signals · 턴 집계     ← §3-1·2 필요
  ③ LLM 판별: user_message + 구조 신호 → 원인 진단
  ④ 개선 제안을 현재 컨텍스트에 출력 → 사용자 승인 시
     그 자리에서 CLAUDE.md/스킬/워크플로우에 반영
  ⑤ (선택) 분석 마찰을 wimcc feedback 문서에 축적
```

**역할 분담**:

- **wimcc MCP (데이터 계층, read-only)**: 이미 존재하는 `/mcp` Streamable HTTP를
  그대로 사용. 회고에 필요한 것은 새 서버가 아니라 §3의 집계 도구 추가.
- **skill (오케스트레이션 계층)**: 분석 절차(세션 식별 → 집계 수집 → 판별 →
  제안 → 반영)를 지시하는 프롬프트 워크플로우. wimcc에 쓰기 API를 요구하지
  않으므로 read-only 원칙과 충돌 없음.
**배포 구조 (결정, 2026-06-12)**: wimcc 리포가 Claude Code plugin 마켓플레이스를
겸한다. 스킬 본체는 Agent Skills 업계 표준 레이아웃(리포 최상위 `skills/`)을
SSOT로 두고, plugin은 심링크로 참조한다 — 향후 Claude Code 외 에이전트 제품으로의
확장을 고려한 구성.

```
whats-in-my-cc/
├── src/                                  # 바이너리 소스 (채널 1: crates.io/릴리스)
├── skills/                               # Agent Skills 표준 레이아웃 = SSOT
│   └── session-retrospect/
│       ├── SKILL.md
│       └── references/
├── .claude-plugin/
│   └── marketplace.json                  # 이 리포 = plugin 마켓플레이스 선언
└── plugins/session-retrospect/           # 채널 2: Claude Code plugin 시스템
    ├── .claude-plugin/plugin.json
    ├── .mcp.json                         # 127.0.0.1:7878/mcp 자동 등록
    └── skills/session-retrospect → ../../../skills/session-retrospect  (symlink)
```

- 사용자 흐름: 바이너리 설치 → `claude plugin marketplace add <repo>` →
  `/plugin install` — 스킬과 MCP 접속 설정이 한 번에 들어간다. `~/.claude` 쓰기
  주체는 Claude Code 자신이므로 non-goal(설정/skill 변경 금지)과 충돌 없음.
- 버전 skew(plugin은 git 추적, 바이너리는 릴리스)는 스킬 Step 0의
  schema_version 핸드셰이크가 안전망.
- 검토한 대안: 별도 스킬 리포 + GitHub Actions 동기화(성립하나 가동부 多),
  바이너리 임베드 + `wimcc skill install`(installer 구현 필요·non-goal 예외
  명문화 필요) — 둘 다 기각.
- **구현 전 검증 필요(Real-data anchoring)**: ① marketplace.json의 리포 내 경로
  참조 스키마, ② plugin install 파이프라인이 심링크된 skills/ 디렉토리를
  따라가는지 smoke 테스트(불가하면 심링크 대신 CI 복사 스텝으로 대체),
  ③ Windows 클론 시 symlink 제약은 알려진 한계로 문서화.

**선행 조건 정리**: §3-3(프로젝트→세션 매핑)이 없으면 사용자가 세션 ID를 손으로
찾아야 하므로 이것이 1순위. §3-1·2는 없어도 동작은 하나(이번처럼 전량 수집 후
로컬 집계) 분석 비용이 크게 줄어든다.

## 6. 우선순위 제안

| 순위 | 항목 | 근거 |
|------|------|------|
| 1 | §3-3 프로젝트→세션 매핑 | 회고 skill의 전제 조건 |
| 2 | §3-1 kind 필터 · §3-2 턴 집계 | 분석 마찰의 최대 원천 |
| 3 | §5 session-retrospect skill 작성 | 1·2 위에서 즉시 가동 가능 |
| 4 | §4 Signal 품질 3건 | 저비용 수정 |
| 5 | §2 Signal 규칙 2종 | 다른 세션에서 패턴 확인 후 |
