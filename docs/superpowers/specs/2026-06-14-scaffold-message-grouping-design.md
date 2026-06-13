# 우측 scaffold(커맨드·스킬) 메시지 모아보기 설계

- 날짜: 2026-06-14
- 상태: 설계(브랜치 `claude/ui-improvements-k1pcis`에서 이어서 구현)
- 관련: `2026-06-13-subagent-parallel-batch-grouping-design.md` §9(분리 슬라이스로 명시한 그 항목). 배치 그룹핑과 동형 패턴(연속 항목 접이식 묶음).

## 1. 문제

merge된 메시지 출처 분류(`messageOrigin.ts`) 이후, CC가 `type:"user"`로 접던 슬래시커맨드·스킬 scaffold·command-output·system-interrupt가 **우측(유저측)에 개별 카드**로 렌더된다. 연속 주입(실측 세션 `5bde98d8`: `[Request interrupted]`(system) → Caveat(command-output) → `/chrome`(command) → 출력(command-output) → `/claude-in-chrome`(command) → skill body)이 6개 카드로 스트림을 어지럽힌다. 대화의 본류(사람 입력·어시스턴트)가 묻힌다.

## 1b. 선행 — task-notification 분류 빈틈 (실측 발견)

merge된 `messageOrigin`은 슬래시커맨드·스킬·command-output·interrupt만 마커로 잡는다. 하지만 하네스가 **백그라운드 작업 완료 알림을 `<task-notification>`으로 시작하는 user 역할 메시지로 주입**하는데, 이 마커가 없어 기본 `'human'`으로 분류 → "You"로 우측 게시(사용자가 안 친 것). 실측: 전 DB에서 `<task-notification>`-선행 user_message **55건**, 전부 isMeta 없음. (반복 패턴, 표본 1 아님.)

**고침:** `messageOrigin`에 새 출처 `'notification'` + 마커 `/^\s*<task-notification>/` 추가. MessageCard는 라벨 "알림"(아이콘 Bell), sourceTag `'notification'`, bubble은 `.metaBubble`(violet scaffold) — 사람 "You"와 구분. `'notification'`도 `origin !== 'human'`이라 아래 scaffold 모아보기에 자동 포함.

## 2. 설계

배치 그룹핑과 같은 접이식 패턴을 재사용:

- **streamModel 후처리 `groupScaffold(items)`**: 최상위 `items`를 스캔해, **연속된 user-side scaffold MessageItem ≥2개**(`role==='user' && origin && origin !== 'human'`)를 `ScaffoldGroup`으로 래핑한다. 단일 scaffold 메시지는 인라인 유지(불필요한 래퍼 방지). 경계로 런을 끊는 것: 사람 메시지(origin==='human'), assistant/thinking/activity-run/sidechain-group/batch-group 등 scaffold 아닌 모든 항목.
- **타입**: `ScaffoldGroup{type:'scaffold-group', id, items: MessageItem[], commandNames: string[]}`. `commandNames` = 그룹 내 `origin==='command'` 항목의 `commandName`(없으면 빈 배열). `StreamItem` union에 추가.
- **컴포넌트 `ScaffoldGroup.tsx`**: 
  - 접힘(기본) = violet 칩 "커맨드·스킬"(scaffold 정체성, `--wimcc-scaffold`) + 개수 + 미리보기(commandNames 일부 + 나머지 출처 힌트, 예: "/chrome /claude-in-chrome +시스템·출력 4"). 
  - 펼침 = 개별 `MessageCard`(현재 렌더 그대로).
  - 접기 패턴은 `SubagentGroup`/`BatchGroup`과 동일(userOverride state, containsSelected 자동 펼침). scaffold는 "참조, 대화 아님"이라 **기본 접힘**.
  - `data-testid="scaffold-group"`+`data-expanded`, `scaffold-toggle`, `scaffold-preview`.
- **ConversationStream**: `renderItem`에 `scaffold-group` → `<ScaffoldGroup>`; `itemContainsEvent`에 재귀(`item.items.some(...)`).

## 3. 범위 밖 (v1)
- subagent 내부(SidechainGroup.items)·배치 내부의 scaffold 그룹핑은 하지 않음(최상위 main 흐름만). subagent 그룹은 이미 접이식이라 중복 회피.

## 4. degrade
- origin 없는(pre-messageOrigin) user 메시지는 'human' 취급이라 그룹 대상 아님 — 안전.

## 5. Tests (TDD red 우선)
- `groupScaffold`: 연속 scaffold ≥2 → `scaffold-group`(items 보존, commandNames 수집) · 단일 scaffold → 인라인 유지 · 사람/어시스턴트 메시지가 런을 끊음 · scaffold 사이 assistant가 끼면 두 그룹.
- `ScaffoldGroup.tsx`: 접힘 시 미리보기·개수 보이고 개별 카드 숨김 / 펼침 시 카드 N개 / 선택이 그룹 내부면 자동 펼침.
- ConversationStream: scaffold-group 렌더·스크롤 타깃.
- 합성 fixture + 실데이터 근거 주석(세션 5bde98d8의 6-카드 런).
