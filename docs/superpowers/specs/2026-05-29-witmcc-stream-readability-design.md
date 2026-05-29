# 대화 스트림 가독성 재설계 — Design Spec

> 2026-05-29 · 세션 상세 화면의 좌측 대화 스트림 + 우측 Insight/상세 패널 + 노드 라벨 재설계.
> 선행 트랙: `2026-05-29-witmcc-ux-redesign-v2-design.md` (R1~R7, 레이아웃·타임라인·subgraph 골격). 이 문서는 그 위에서 "스트림이 휴먼 리더블하지 않다"는 후속 피드백을 해소한다.

## 1. 문제 (실데이터 조사 결과)

사용자 피드백: 좌측 스트림이 (a) 빈 카드가 쌓이고 (b) 채워진 것도 명령/스킬 scaffolding이라 읽히지 않으며, (c) 사용자/AI/추론/도구가 시각적으로 구분되지 않고, (d) 도구 호출이 세로 공간을 독식한다. 우측 패널은 (e) 노드가 타입+해시 id만 보여 의미 불명, (f) 하단 공간 낭비.

DB 전수 조사(`.witmcc.sqlite`)로 확인한 사실:

- **빈 카드는 실제 파싱 버그다.** `user_message` 정규화 payload는 두 형태로 갈린다: `{content:"…"}` 2,011건 vs `{content_ordinal,text:"…"}` **7,971건**. 현재 `streamModel.ts`는 user_message preview를 `p.content`만 읽어, `text` 형태 7,971건이 **전부 빈 카드**로 렌더된다. (`assistant_message`는 `p.text`를 올바르게 읽어 빈 카드 0건, `tool_name` null 0건 — Assistant·Tool 자체는 비어있지 않다.)
- **채워 넣어도 대부분 비-휴먼 콘텐츠다.** 그 `text` 값의 실제 내용: `"Base directory for this skill: …"` 4,056건, `<command-message>…</command-message>` 류 슬래시명령 scaffolding 수천 건, `"[Request interrupted by user]"` 92건. Claude Code가 user 역할로 주입하는 스킬/명령/시스템 텍스트이지 사람 입력이 아니다.
- **redacted thinking.** 표본 세션(`0056c8f5…`)의 thinking 24건은 24건 모두 `"(thinking redacted)"` — redaction gate가 raw thinking을 저장하지 않으므로 빈 껍데기 카드.
- **노드 라벨 재료는 존재한다.** model명은 raw transcript `message.model`에 존재(`claude-opus-4-8`, `claude-sonnet-4-6`, `claude-haiku-4-5` 등 실재). 단 정규화 `assistant_message`/graph_node payload에는 없다. `hook_event`는 `hookName`(예 `PreToolUse:Agent`, `SessionStart:clear`) 보유. `tool_call` graph_node는 `tool_name`을 직접 갖지 않고 `merge_keys.tool_use_id` + payload `input`만 가진다(라벨에 tool_name 필요 → 백엔드 surfacing).

결론: **버그 수정 + 노이즈 분류/축약 + 라벨 파생**을 함께 한다.

## 2. 이벤트 분류 (스트림의 1급 시민 / 축약 / 제외)

`buildStreamCards`를 분류기로 재작성한다. 각 이벤트는 정확히 한 범주로:

- **1급 메시지 (개별 버블):**
  - `user_message` 중 **사람 입력** — content/text 둘 다 읽고, scaffolding 패턴이 아닌 것.
  - `assistant_message` — text 보유(전부 해당).
  - `thinking` 중 **redacted 아닌 것** (휴먼 리더블한 추론).
- **축약 (activity 스택으로 흡수):** `tool_call`·`tool_result`·`hook_event`·`session_state`·`metric_sample`·`otel_span`·`log_record`·`attachment_meta`, 그리고 **redacted thinking**.
- **제외 (스트림에 렌더 안 함):** content/text가 빈 user_message, scaffolding user_message(아래 패턴), `system_summary`(KPI/메타로 이미 표현).

**scaffolding 판정 (user_message):** content/text가 다음으로 시작/구성되면 scaffolding으로 분류 — `<command-name>`, `<command-message>`, `<command-args>`, `<local-command-stdout>`, `<local-command-caveat>`, `Base directory for this skill:`, `<system-reminder>` 전용, `[Request interrupted…]`. 이미지 placeholder(`[Image: …]`)도 비-입력으로 본다. **이들은 제외가 아니라 축약 스택에 흡수**(완전 소실 방지). 판정 규칙은 fixture로 잠근다(§7).

`tool_result`는 이미 별도 kind(actor=system)로 분리되어 있으므로, 매칭되는 `tool_call`과 묶어 activity 항목의 결과(ok/error)로 표현한다.

## 3. 스트림 레이아웃 (채팅 구조)

- **정렬:** 사용자 메시지 = **우측 정렬** 버블(accent 배경/테두리). assistant = **좌측 정렬** 버블(surface). 휴먼 리더블 thinking = 좌측, 점선 좌측 테두리 + 이탤릭 + muted로 답변과 **명확히 구분**.
- **역할 표현 (확정안 A):** 아이콘 아바타(lucide 라인 아이콘, 이모지 아님) + 라벨. user = `You` + user 아이콘. assistant = **실제 모델명** `Claude Opus 4.8` 등 + Claude 마크 아이콘. 모델명은 §6에서 surfacing.
- **순서·스크롤:** 기존(시간순, 최신 하단, 가상화, 선택 카드 scroll-into-view, append autoscroll) 유지. 카드 종류만 위 분류로 바뀐다.
- **메모리:** 분류로 1급 카드 수가 크게 줄고(노이즈가 축약 스택 1줄로 접힘), 기존 §7 가상화·fallback 캡 유지.

## 4. Activity 스택 (확정안 B — phase 분할)

메시지 사이의 모든 축약 대상 이벤트 런(run)을 **episode phase 경계로 1~2개 스택**으로 묶는다(런이 단일 phase면 1개, phase가 바뀌면 분할, 상한 2개 — 초과 시 마지막 스택에 합산).

- **요약 줄:** `[phase badge] 대표도구 — 짧은맥락   ⚠N(에러)   N건 · 소요 ▸`. 대표 도구 = 빈도 상위 1~2 + 반복은 `×N`(예 `Read ×17`). phase badge는 기존 phase 색.
- **펼침:** 스택 클릭 시 **인라인** 항목 목록(도구명 + 핵심 인자 + ok/error). 길면 "+N건 더".
- **포커스:** 항목 클릭 시 그 노드를 선택 → 우측 패널(§5)에 상세. 즉 activity 항목도 graph node와 연결(`source_event_ids`/`tool_use_id` → node).
- **시도 후 조정:** phase 분할이 인사이트를 주는지 실제 화면에서 재평가(사용자 합의). 안 맞으면 "런당 1개"로 후퇴 — 분할 규칙을 한 함수로 격리해 교체 쉽게.

## 5. 우측 패널 (확정안 A — 탭 2개)

탭을 **Insight / Raw** 둘로 축소한다(기존 Detail 탭 내용은 Insight 하단으로 흡수).

**Insight 탭 = 세로 3섹션:**
1. **인과 이웃 (compact subgraph):** 기존 FocusedInsightGraph, 노드는 §6 요약 라벨 사용, hop 토글 유지. 높이 축소.
2. **포커스 노드 상세 (지금 낭비되는 공간):** 종류별로 채움.
   - 헤더: 종류 아이콘 + 라벨 + (해시 id, 작게·복사가능).
   - 공통: 시각·소요·episode.
   - `tool_call`: **파라미터 전체**(key-value; 긴 값(Bash command 등)은 코드블록, 경로는 mono) + **결과**(ok/error 배지·exit·content preview·소요).
   - `assistant_message`: 전체 메시지 텍스트 + 토큰(in/out/cache).
   - `hook_event`: hookName·hookEvent·stdout/exitCode.
   - `user_message`: 전체 텍스트(scaffolding이면 그대로지만 라벨로 명시).
   - `otel_span`: span name·duration·attributes 요약.
3. **이 노드의 finding:** evidence-linked findings(severity·요약·confidence). 없으면 섹션 생략.

**Raw 탭:** 기존 JsonTree(펼침 지속성 유지).

선택 없을 때: Insight 탭에 "노드를 선택하세요" 힌트(기존). 기본 탭은 Insight.

## 6. 노드 라벨 + 백엔드 surfacing

**라벨 형식 (timeline 툴팁 + subgraph 노드 + activity 항목 + 상세 헤더가 모두 동일 라벨 사용):**

| node_kind | 라벨 | 출처 |
|---|---|---|
| tool_call | `Read · slide_logo-17.jpg` (tool_name + 핵심 인자) | tool_name(§6 surfacing) + payload.input(file_path/command/skill/pattern) |
| assistant_message | `Opus 4.8 · "메시지 앞부분…"` | model(§6) + payload.text |
| user_message | `You · "메시지…"` / scaffolding은 `command · /plugin` | payload.content\|text + 분류 |
| thinking | `추론 · …` (redacted면 축약스택에만) | payload.thinking |
| hook_event | `hook · PreToolUse:Agent` | payload.hookName |
| otel_span | `span · claude_code.interaction` | payload.raw_span.name |
| verification_run | `verify · …` | verification 요약 |
| diff_hunk | `diff · path:Lx-y` | payload |

해시 id는 라벨에서 제거, 상세 헤더에서만 노출.

**백엔드 변경 (graph builder + DTO):** GraphNode에 파생 필드 두 개를 추가해 단일 출처로 만든다 —
- `label: string` (위 규칙으로 빌더가 계산; 종류별 derivation은 Rust 측에 두어 timeline/subgraph/상세가 일관).
- `tool_name: string | null` 및 `model: string | null`을 노드에 surfacing(라벨·상세·역할표현 공용). model은 assistant 노드 한정, raw transcript `message.model`에서.

`GraphNode.schema_version` bump + migration. API `GraphNodeDto`에 `label`/`tool_name`/`model` 추가. 프론트는 가능하면 백엔드 `label`을 그대로 쓰고, 없을 때만(구버전 데이터) payload에서 파생하는 fallback 유지(source-preserving). **OTel-first/source-preserving 원칙 준수**: 원시 payload는 보존, 라벨은 파생 메타.

## 7. 실데이터 앵커링 (필수)

- scaffolding 판정·이벤트 분류·라벨 파생은 `tests/fixtures/**/real/`에 동결된 실 payload(또는 본 spec에 인용한 실 카운트)에 대한 invariant assertion으로 잠근다. 표본 1건 일반화 금지.
- 최소 fixture: `{content}`형/`{content_ordinal,text}`형 user_message, scaffolding 각 패턴, redacted thinking, tool_call(Read/Bash/Skill), hook_success(hookName), assistant(model 포함 raw).

## 8. 테스트 (TDD red 우선)

- **분류기**(`buildStreamCards` 재작성): kind→범주 매핑, scaffolding 패턴, 빈 카드 제외, text/content 양쪽 읽기(7,971 버그 회귀 잠금), tool_result 병합.
- **activity 그룹핑**: 런→phase 분할(1개/2개/상한), 요약 줄 집계(대표 도구·×N·에러 카운트), 항목→노드 연결.
- **라벨 파생**(순수 함수): 종류별 입력→라벨, 인자 키 선택, 길이 truncation, scaffolding 라벨.
- **레이아웃**(jsdom): user 우측/assistant 좌측/thinking 구분 data-* 속성, 역할 라벨에 모델명, activity 스택 펼침·포커스→선택, 우측 패널 2탭·포커스 노드 상세 섹션·tool 파라미터 렌더.
- **백엔드**: graph builder label/tool_name/model 산출 단위 테스트 + migration round-trip.
- **브라우저 smoke**: 실세션에서 가독성·정렬·축약·라벨·우측 상세 확인(CLAUDE.md 의무).

## 9. Non-goals

- transcript/hook/OTel 수집·정규화 의미 변경(분류·라벨은 파생 레이어에서). 단 §6의 model/tool_name surfacing·node label은 graph builder에 추가하는 파생 필드이며 원시 보존을 깨지 않는다.
- redacted thinking 복원(불가·금지).
- 사용자 실명 표시(transcript에 없음 → `You`).
- annotation/correction write(전 트랙과 동일 금지).

## 10. 열린 질문

- phase 분할 vs 런당 1개: 화면 보고 재평가(§4). 분할 규칙 격리로 전환 비용 최소화.
- model 라벨 표기: `Claude Opus 4.8` 풀네임 vs `Opus 4.8` 약식 — 구현 시 폭 보고 결정.
- activity 상한 2개 초과 런의 합산 방식(마지막에 합산) — 실데이터에서 과도하게 큰 런이 있으면 재검토.
