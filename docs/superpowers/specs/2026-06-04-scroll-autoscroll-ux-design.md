# 세션 상세 스크롤/오토스크롤 UX 재설계 (2026-06-04)

## 배경

`ConversationStream`(react-virtual 기반 메시지 뷰)의 스크롤 동작에서 사용자 관측 문제:

1. **이전 메시지 로드의 "2회 dance"**: 최상단(≤240px) 도달 시 1회 로드되지만,
   prepend 후 위로 더 스크롤할 여유가 없어 다음 페이지 재트리거가 안 됨 →
   "아래로 내렸다 다시 위로" 동작 필요. 또 진짜 세션 시작인지 직관적으로 모름.
2. **오토스크롤 상태 불투명**: 지금 자동추적 중인지/최상단·최하단인지 알 수단 부재.
3. (선행 조사) **암묵적 오토스크롤의 점프·깜박임**(이슈 2·3): react-virtual
   `followOnAppend` + 2초 `followInitRef` bottom-pin + onScroll loadOlder가 경쟁 →
   tip에서 멀리 있어도 append 시 tip으로 끌려가거나(점프), tip 근처 jitter.

## 결정 (사용자 승인)

- **통합**: 명시적 stick-to-bottom(오토스크롤) 모델을 단일 source of truth로 도입하여
  기존 암묵적 메커니즘(`followOnAppend`, `followInitRef` bottom-pin)을 대체.
  이슈 2·3을 같은 모델로 해소.
- **이전 로드**: seamless prefetch(최상단 도달 *전* 미리 로드·prepend) +
  세션 시작 도달 시 "대화 시작" 마커.
- **컨트롤**: 스트림 우하단 **상시 상태 pill**. ON=`● 실시간`, OFF=`↓ 최신`(+신규 N badge).
- **아키텍처(A1)**: react-virtual은 prepend 안정화 전용(`anchorTo:'end'` 유지,
  `followOnAppend:false`). 자동추적은 새 `useAutoscroll` 컨트롤러가 명시적으로 통제.

## 유닛 경계

### `useAutoscroll(scrollRef, { itemsSignature })` — 신규 훅
스트림 스크롤 위치 정책의 단일 소유자. react/DOM만 의존 → 단위테스트 가능.

상태/동작:
- 초기 mount: `scrollToBottom()` + `autoscroll=true`.
- scroll 리스너(`onScroll`): 바닥에서 `BOTTOM_THRESHOLD`(80px) 이내면 `autoscroll=true`,
  `newCount=0`. 위로 벗어나면 `autoscroll=false`.
- items 변경(`onItemsChanged(prev,next)` 또는 itemsSignature 변화):
  - tip append(마지막 item id 변경): `autoscroll`이면 `scrollToBottom()`,
    아니면 `newCount += (append 개수)`.
  - prepend(첫 item id 변경, 마지막 동일): 무시(react-virtual anchor가 위치 유지).
- `enable()`(pill OFF→클릭): `scrollToBottom()` + ON + `newCount=0`.
- `disable()`(pill ON→클릭): OFF(현 위치 유지).
- `scrollToBottom`은 instant(`behavior:'auto'`) — measurement jitter/jank 방지.

반환: `{ autoscroll, newCount, enable, disable, onScroll, onItemsChanged }`.

### `AutoscrollPill` — 신규 표시 컴포넌트
입력만 받아 그림(로직 없음). 우하단 `position:absolute`.
- ON: `● 실시간`. OFF: `↓ 최신` + `newCount>0`이면 badge.
- 클릭: ON→`disable`, OFF→`enable`. `<button aria-pressed={autoscroll}>`.

### `ConversationStream` — 조립
- `useVirtualizer`: `anchorTo:'end'` 유지, **`followOnAppend:false`**.
  **`followInitRef` bottom-pin 효과/2초 타이머 제거.**
- `useAutoscroll(parentRef, …)` 사용. onScroll에서 autoscroll.onScroll() + 기존 loadOlder.
- `<AutoscrollPill>` 렌더. `!canLoadOlder`면 스트림 최상단에 "대화 시작" 마커.

### `scrollAnchor` / `useSessionWindow` — prefetch
- `LOAD_OLDER_TOP_PX` 240 → **약 1 뷰포트(또는 800px)** 로 확대(prefetch ahead).
  "scrolling up" 가드는 유지(anchor 재고정의 self-retrigger 방지).
- `canLoadOlder = oldest !== null`(기존). `!canLoadOlder` → "대화 시작" 마커 조건.

## 데이터 흐름
SSE envelope → `useSessionWindow.loadNewer`(append at tip) → items 변경 →
`useAutoscroll.onItemsChanged`: ON이면 scrollToBottom, OFF면 newCount++.
사용자 scroll-up near top → loadOlder(prefetch) → prepend → react-virtual anchor가 위치 유지.

## 엣지 케이스
- OFF 중 사용자가 직접 바닥까지 스크롤 → atBottom 감지 → ON + newCount reset(요구사항).
- 빈 스트림 → pill 숨김(items 0).
- prepend와 append가 같은 틱에 섞이는 경우는 실무상 분리 이벤트라 first/last id로 구분.

## 테스트 (TDD red 우선)
- `useAutoscroll` 단위: 초기 ON / 위로→OFF / 바닥복귀→ON+reset / OFF중 append→newCount++ /
  ON중 append→scrollToBottom 호출 / enable·disable.
- `AutoscrollPill`: ON·OFF 라벨·badge·클릭 핸들러·aria.
- `scrollAnchor`: 확대된 prefetch 임계값.
- `ConversationStream`: 옵션 계약(`followOnAppend:false`, `anchorTo:'end'`), pill 렌더,
  `!canLoadOlder` 시 "대화 시작" 마커.
- 브라우저 smoke: 실제 스크롤·점프·jitter·연속 prepend·pill 동작(jsdom 레이아웃 불가분).

## 비목표
- 무한 가상 스크롤 외 페이징 정책 변경 없음(window 크기·LRU 유지).
- 스크롤바 외형(별도 완료된 작업) 변경 없음.
