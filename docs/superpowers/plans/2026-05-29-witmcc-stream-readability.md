# 대화 스트림 가독성 재설계 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 좌측 대화 스트림을 채팅 구조로, 노드/카드를 휴먼 리더블 라벨로, 우측 패널을 2탭(Insight 상세 + Raw)으로 재설계한다. 빈 카드 파싱 버그(`text` 미파싱)와 scaffolding 노이즈를 함께 잡는다.

**Architecture:** (1) 백엔드 정규화 payload에 `tool_name`·`model`을 추가(JSON payload라 migration 불필요, re-ingest 필요) → graph node가 자동 상속. (2) 프론트 단일 라벨 deriver `nodeLabel()`로 timeline·subgraph·activity·상세가 일관. (3) `buildStreamModel()` 분류기가 이벤트를 message(1급)/activity(축약)/제외로 나누고 phase로 activity를 1~2 스택 그룹핑. (4) 채팅 레이아웃 + 2탭 패널.

**Tech Stack:** Rust(ingest/graph) + sqlx, React 18 / TS / Vitest + Testing Library, lucide-react, @xyflow/react.

**Spec:** `docs/superpowers/specs/2026-05-29-witmcc-stream-readability-design.md`

**실행 순서:** S1(백엔드) → S4(라벨 deriver) → S2(분류기) → S3(activity 그룹핑) → S5(채팅 레이아웃) → S6(2탭 패널) → S7(smoke+PR). S4는 S5/S6보다 먼저(둘 다 라벨 사용).

---

## File Structure

**백엔드 (S1):**
- Modify `src/ingest/mapping.rs` — tool_call payload에 `tool_name`, assistant_message payload에 `model` 추가.
- Modify `tests/graph_build.rs` 또는 신규 `tests/payload_enrichment.rs` — 정규화 payload invariant.
- Fixture `tests/fixtures/transcripts/minimal_session.jsonl` (이미 `model:"claude-opus-4-7"` 보유) 활용.

**프론트 (S2~S6):**
- Create `webui/src/components/replay/stream/nodeLabel.ts` (+test) — 종류별 라벨 deriver (S4).
- Rewrite `webui/src/components/replay/stream/streamModel.ts` (+test) — `buildStreamModel()` 분류기 (S2).
- Create `webui/src/components/replay/stream/activityGroup.ts` (+test) — phase 분할 그룹핑 (S3).
- Rewrite `webui/src/components/replay/stream/StreamCard.tsx`, `ConversationStream.tsx` (+tests) — 채팅 레이아웃 + activity 스택 (S5).
- Create `webui/src/components/replay/stream/ActivityStack.tsx` (+test) — 축약 스택 컴포넌트 (S5).
- Rewrite `webui/src/components/replay/detail/DetailPanel.tsx`, `InsightTab.tsx` (+tests); Create `NodeDetail.tsx` (+test); delete `DetailTab.tsx` (S6).
- Modify `webui/src/routes/SessionDetailPage.tsx` — 새 stream model·props 배선 (S2/S5/S6).
- Modify `webui/src/api/types.ts` — `ObservedEventDto.payload` 주석(model/tool_name), GraphNode payload 동일.

---

## S1 — 백엔드: 정규화 payload에 tool_name·model 추가 (Rust, TDD)

목표: tool_call 노드가 라벨에 쓸 `tool_name`을, assistant 노드가 `model`을 payload에 갖게 한다. graph node는 `e.payload.clone()`이라 자동 상속. **migration 없음**(payload는 TEXT JSON). dev DB는 `witmcc init-db` + re-ingest 필요(문서화).

### Task S1.1: tool_call payload에 tool_name

**Files:** Modify `src/ingest/mapping.rs:184-189`; Test `tests/payload_enrichment.rs` (create).

- [ ] **Step 1: 실패 테스트 작성** — `tests/payload_enrichment.rs`:

```rust
// tests/payload_enrichment.rs
use witmcc::ingest::mapping; // adjust to crate's actual module path (see existing tests/graph_build.rs imports)

#[test]
fn tool_call_payload_carries_tool_name() {
    // A raw assistant message content item of type tool_use.
    let item = serde_json::json!({
        "type": "tool_use", "id": "toolu_1", "name": "Read",
        "input": {"file_path": "/tmp/a.jpg"}
    });
    let ev = mapping::map_content_item(&item, 0 /* ordinal */); // use the real entry point; see Step 3
    assert_eq!(ev.tool_name.as_deref(), Some("Read"));
    assert_eq!(ev.payload["tool_name"], serde_json::json!("Read"));
    assert_eq!(ev.payload["input"]["file_path"], serde_json::json!("/tmp/a.jpg"));
}
```

> NOTE to implementer: `tests/graph_build.rs` shows the real crate import path and how ObservedEvent is constructed in tests — mirror it. If `map_content_item` is not directly callable, write the test against the public ingest entry that produces ObservedEvents from a transcript line (use `minimal_session.jsonl` fixture) and assert the resulting tool_call event's `payload["tool_name"]`.

- [ ] **Step 2: 실패 확인** — `cargo test --test payload_enrichment tool_call_payload_carries_tool_name`. Expected: FAIL (payload has no `tool_name`).

- [ ] **Step 3: 구현** — `src/ingest/mapping.rs` tool_use 분기(현재 line 184-189):

```rust
"tool_use" => {
    e.kind = EventKind::ToolCall;
    e.tool_use_id = item.get("id").and_then(|x| x.as_str()).map(String::from);
    e.tool_name = item.get("name").and_then(|x| x.as_str()).map(String::from);
    e.payload = json!({
        "content_ordinal": ord,
        "tool_name": e.tool_name,           // <-- 추가 (null이면 JSON null)
        "input": item.get("input")
    });
}
```

- [ ] **Step 4: 통과 확인** — `cargo test --test payload_enrichment`. Expected: PASS.

- [ ] **Step 5: commit** — `git add src/ingest/mapping.rs tests/payload_enrichment.rs && git commit -m "ingest: carry tool_name into tool_call payload for node labels"`

### Task S1.2: assistant_message payload에 model

**Files:** Modify `src/ingest/mapping.rs` (`map_assistant`, ~141-198); Test `tests/payload_enrichment.rs`.

- [ ] **Step 1: 실패 테스트 추가** — `tests/payload_enrichment.rs`:

```rust
#[test]
fn assistant_message_payload_carries_model() {
    // Drive the assistant normalizer with a raw message that has a model.
    let raw = serde_json::json!({
        "type": "assistant",
        "message": {"id":"msg_1","model":"claude-opus-4-7",
                    "content":[{"type":"text","text":"hi"}]}
    });
    let events = mapping::map_assistant_record(&raw); // use the real assistant mapping entry
    let text_ev = events.iter().find(|e| e.kind_is_assistant_message()).unwrap();
    assert_eq!(text_ev.payload["model"], serde_json::json!("claude-opus-4-7"));
    assert_eq!(text_ev.payload["text"], serde_json::json!("hi"));
}
```

> NOTE: adjust `map_assistant_record` / `kind_is_assistant_message` to the real API in mapping.rs. Simplest robust path: run the full transcript ingest on `tests/fixtures/transcripts/minimal_session.jsonl` (model `claude-opus-4-7`) and assert the assistant_message ObservedEvent payload has `model`.

- [ ] **Step 2: 실패 확인** — `cargo test --test payload_enrichment assistant_message_payload_carries_model`. Expected: FAIL.

- [ ] **Step 3: 구현** — `map_assistant`에서 message의 model을 추출해 각 text 블록 payload에 주입:

```rust
// near the top of map_assistant, after binding the message object:
let model = msg.get("model").and_then(|m| m.as_str()).map(String::from);
// when building a text-block assistant_message event:
e.payload = json!({"content_ordinal": ord, "text": text, "model": model});
```

> tool_use 블록(=tool_call)에는 model 불필요(라벨은 tool_name 사용). text 블록(assistant_message)에만 추가.

- [ ] **Step 4: 통과 확인** — `cargo test --test payload_enrichment`. Expected: PASS.

- [ ] **Step 5: 전체 백엔드 회귀** — `cargo test 2>&1 | tail -20`. 기존 graph_build/transcript 테스트가 payload 모양 변화로 깨지면(예: 정확 payload 비교) 새 키를 반영해 갱신. Expected: all green.

- [ ] **Step 6: commit** — `git add src/ingest/mapping.rs tests/payload_enrichment.rs && git commit -m "ingest: carry model into assistant_message payload for role labels"`

### Task S1.3: 구현 노트 + re-ingest 문서화

- [ ] **Step 1:** `docs/implementation-notes.html`에 섹션 추가: payload enrichment(tool_name/model), migration 없음, dev DB는 `witmcc init-db` + re-ingest 필요. CLAUDE.md Status의 운영 주의에 한 줄.
- [ ] **Step 2: commit** — `git add docs/implementation-notes.html CLAUDE.md && git commit -m "docs: note payload enrichment + re-ingest requirement"`

---

## S4 — 프론트: 노드/카드 라벨 deriver (TDD)

순수 함수 하나로 timeline 툴팁·subgraph·activity 항목·상세 헤더·stream 역할 라벨이 일관되게 쓴다. graph node payload(이제 tool_name/model 보유)와 ObservedEvent 양쪽에서 동작하도록 입력은 `{node_kind, payload}` 최소 형태.

### Task S4.1: nodeLabel 순수 함수

**Files:** Create `webui/src/components/replay/stream/nodeLabel.ts` + `__tests__/nodeLabel.test.ts`.

- [ ] **Step 1: 실패 테스트** — `nodeLabel.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { nodeLabel, formatModel } from '../nodeLabel';

const L = (node_kind: string, payload: unknown) => nodeLabel({ node_kind, payload });

describe('formatModel', () => {
  it('shortens known model ids', () => {
    expect(formatModel('claude-opus-4-8')).toBe('Opus 4.8');
    expect(formatModel('claude-sonnet-4-6')).toBe('Sonnet 4.6');
    expect(formatModel('claude-haiku-4-5-20251001')).toBe('Haiku 4.5');
  });
  it('falls back for synthetic/unknown', () => {
    expect(formatModel('<synthetic>')).toBe('Claude');
    expect(formatModel(null)).toBe('Claude');
  });
});

describe('nodeLabel', () => {
  it('tool_call: tool name + key arg', () => {
    expect(L('tool_call', { tool_name: 'Read', input: { file_path: '/a/slide_logo-17.jpg' } }))
      .toEqual({ kind: 'tool', primary: 'Read', secondary: 'slide_logo-17.jpg' });
    expect(L('tool_call', { tool_name: 'Bash', input: { command: 'rm -f x.jpg && ls' } }))
      .toEqual({ kind: 'tool', primary: 'Bash', secondary: 'rm -f x.jpg && ls' });
    expect(L('tool_call', { tool_name: 'Skill', input: { skill: 'corp-pptx-style' } }))
      .toEqual({ kind: 'tool', primary: 'Skill', secondary: 'corp-pptx-style' });
  });
  it('assistant_message: model + message head', () => {
    expect(L('assistant_message', { model: 'claude-opus-4-8', text: 'NC 브랜드 스타일로 다시 만들겠습니다.' }))
      .toEqual({ kind: 'assistant', primary: 'Opus 4.8', secondary: 'NC 브랜드 스타일로 다시 만들겠습니다.' });
  });
  it('user_message: real text', () => {
    expect(L('user_message', { content: '왜 폰트를 썼지?' }))
      .toEqual({ kind: 'user', primary: 'You', secondary: '왜 폰트를 썼지?' });
  });
  it('user_message: scaffolding becomes command label', () => {
    expect(L('user_message', { content: '<command-name>/plugin</command-name>' }))
      .toEqual({ kind: 'user', primary: 'command', secondary: '/plugin' });
  });
  it('hook_event: hookName from either shape', () => {
    expect(L('hook_event', { hookName: 'PreToolUse:Agent' }).secondary).toBe('PreToolUse:Agent');
    expect(L('hook_event', { hook: { hook_event_name: 'PreToolUse' } }).secondary).toBe('PreToolUse');
  });
  it('otel_span: span name', () => {
    expect(L('otel_span', { raw_span: { name: 'claude_code.interaction' } }).secondary).toBe('claude_code.interaction');
  });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run src/components/replay/stream/__tests__/nodeLabel.test.ts`. Expected: FAIL (module missing).

- [ ] **Step 3: 구현** — `nodeLabel.ts`:

```ts
export interface NodeLabel { kind: 'user' | 'assistant' | 'thinking' | 'tool' | 'hook' | 'span' | 'verify' | 'diff' | 'other'; primary: string; secondary: string; }

const SCAFFOLD = /^\s*(<command-name>|<command-message>|<command-args>|<local-command-stdout>|<local-command-caveat>|Base directory for this skill:|\[Request interrupted)/;

function asObj(v: unknown): Record<string, unknown> { return v && typeof v === 'object' ? v as Record<string, unknown> : {}; }

export function formatModel(raw: unknown): string {
  if (typeof raw !== 'string' || !raw.startsWith('claude-')) return 'Claude';
  const m = raw.match(/^claude-(opus|sonnet|haiku)-(\d+)-(\d+)/);
  if (!m) return 'Claude';
  const fam = m[1][0].toUpperCase() + m[1].slice(1);
  return `${fam} ${m[2]}.${m[3]}`;
}

function toolArg(input: unknown): string {
  const i = asObj(input);
  for (const k of ['command', 'file_path', 'pattern', 'skill', 'path', 'query', 'url']) {
    if (typeof i[k] === 'string') return k === 'file_path' || k === 'path' ? (i[k] as string).split('/').pop()! : i[k] as string;
  }
  return '';
}

export function nodeLabel(node: { node_kind: string; payload: unknown }): NodeLabel {
  const p = asObj(node.payload);
  switch (node.node_kind) {
    case 'tool_call':
      return { kind: 'tool', primary: (p.tool_name as string) || 'tool', secondary: toolArg(p.input) };
    case 'assistant_message':
      return { kind: 'assistant', primary: formatModel(p.model), secondary: (p.text as string ?? '').trim() };
    case 'thinking':
      return { kind: 'thinking', primary: '추론', secondary: (p.thinking as string ?? '').trim() };
    case 'user_message': {
      const txt = (typeof p.content === 'string' ? p.content : (p.text as string)) ?? '';
      if (SCAFFOLD.test(txt)) {
        const name = txt.match(/<command-name>([^<]*)<\/command-name>/)?.[1] ?? 'scaffolding';
        return { kind: 'user', primary: 'command', secondary: name };
      }
      return { kind: 'user', primary: 'You', secondary: txt.trim() };
    }
    case 'hook_event': {
      const hn = (p.hookName as string) ?? asObj(p.hook).hook_event_name as string ?? '';
      return { kind: 'hook', primary: 'hook', secondary: hn };
    }
    case 'otel_span':
      return { kind: 'span', primary: 'span', secondary: (asObj(p.raw_span).name as string) ?? '' };
    case 'verification_run':
      return { kind: 'verify', primary: 'verify', secondary: (p.summary as string) ?? '' };
    case 'diff_hunk':
      return { kind: 'diff', primary: 'diff', secondary: (p.file_path as string) ?? (p.path as string) ?? '' };
    default:
      return { kind: 'other', primary: node.node_kind, secondary: '' };
  }
}
```

- [ ] **Step 4: 통과 확인** — `npx vitest run src/components/replay/stream/__tests__/nodeLabel.test.ts`. Expected: PASS.

- [ ] **Step 5: 실데이터 fixture 잠금** — `tests/payload_enrichment.rs`에서 본 실제 모양과 일치하는지 확인(이미 위 테스트가 실값 사용). 표본 1건 일반화 금지 — 각 kind 최소 1 케이스 + scaffolding 2 패턴.

- [ ] **Step 6: commit** — `git add webui/src/components/replay/stream/nodeLabel.ts webui/src/components/replay/stream/__tests__/nodeLabel.test.ts && git commit -m "webui(stream): nodeLabel deriver — human-readable per-kind labels"`

---

## S2 — 프론트: 스트림 분류기 재작성 (TDD)

`buildStreamCards` → `buildStreamModel`. 출력은 정렬된 `StreamItem[]` where item = `{type:'message', ...}` | `{type:'activity-run', events: ObservedEventDto[]}`. activity-run은 S3에서 phase로 더 쪼갠다(여기선 "비-메시지 연속 런"까지만). content/text 양쪽 읽기로 7,971 버그 회귀 잠금.

### Task S2.1: 이벤트 분류 + 메시지/런 분해

**Files:** Rewrite `webui/src/components/replay/stream/streamModel.ts`; Rewrite `__tests__/streamModel.test.ts`.

- [ ] **Step 1: 실패 테스트** — `streamModel.test.ts` (핵심 케이스):

```ts
import { describe, it, expect } from 'vitest';
import { buildStreamModel } from '../streamModel';
import type { ObservedEventDto } from '../../../../api/types';

function ev(p: Partial<ObservedEventDto> & { event_id: string; kind: string }): ObservedEventDto {
  return { raw_event_id: '', session_id: 's', event_uuid: null, parent_uuid: null,
    observed_at: '2026-05-28T00:00:00Z', actor: 'user', subkind: null, tool_use_id: null,
    tool_name: null, turn_id: null, is_sidechain: false, is_meta: false, payload: {}, ...p } as ObservedEventDto;
}

describe('buildStreamModel', () => {
  it('reads user text from BOTH content and text fields (#bug: 7971 empty cards)', () => {
    const items = buildStreamModel([
      ev({ event_id: 'a', kind: 'user_message', payload: { content: '질문1' } }),
      ev({ event_id: 'b', kind: 'user_message', payload: { content_ordinal: 0, text: '질문2' } }),
    ]);
    const msgs = items.filter((i) => i.type === 'message');
    expect(msgs.map((m: any) => m.text)).toEqual(['질문1', '질문2']);
  });

  it('excludes empty and scaffolding user messages from first-class cards', () => {
    const items = buildStreamModel([
      ev({ event_id: 'a', kind: 'user_message', payload: { text: '' } }),
      ev({ event_id: 'b', kind: 'user_message', payload: { content: '<command-name>/clear</command-name>' } }),
      ev({ event_id: 'c', kind: 'user_message', payload: { content: 'Base directory for this skill: /x' } }),
    ]);
    expect(items.filter((i) => i.type === 'message')).toHaveLength(0);
    // scaffolding/empty are absorbed into an activity-run, not lost:
    expect(items.some((i) => i.type === 'activity-run')).toBe(true);
  });

  it('keeps readable thinking as a message, redacted thinking goes to activity', () => {
    const items = buildStreamModel([
      ev({ event_id: 't1', kind: 'thinking', actor: 'assistant', payload: { thinking: '먼저 확인하자' } }),
      ev({ event_id: 't2', kind: 'thinking', actor: 'assistant', payload: { thinking: '' } }),
    ]);
    const msgs = items.filter((i: any) => i.type === 'message' && i.role === 'thinking');
    expect(msgs).toHaveLength(1);
    expect((msgs[0] as any).text).toBe('먼저 확인하자');
  });

  it('groups a contiguous run of non-message events into one activity-run with its events', () => {
    const items = buildStreamModel([
      ev({ event_id: 'u', kind: 'user_message', payload: { content: 'go' } }),
      ev({ event_id: 'c1', kind: 'tool_call', actor: 'assistant', tool_name: 'Read', payload: { tool_name: 'Read', input: { file_path: '/a' } } }),
      ev({ event_id: 'h1', kind: 'hook_event', actor: 'hook', payload: { hookName: 'PreToolUse' } }),
      ev({ event_id: 'a', kind: 'assistant_message', actor: 'assistant', payload: { text: 'done', model: 'claude-opus-4-8' } }),
    ]);
    expect(items.map((i: any) => i.type)).toEqual(['message', 'activity-run', 'message']);
    expect((items[1] as any).events.map((e: ObservedEventDto) => e.event_id)).toEqual(['c1', 'h1']);
  });

  it('merges tool_result into its tool_call (ok/error) inside the run events', () => {
    const items = buildStreamModel([
      ev({ event_id: 'c1', kind: 'tool_call', tool_use_id: 'x', tool_name: 'Read', payload: { tool_name: 'Read', input: {} } }),
      ev({ event_id: 'r1', kind: 'tool_result', actor: 'system', tool_use_id: 'x', payload: { tool_result: { is_error: true } } }),
    ]);
    const run: any = items.find((i: any) => i.type === 'activity-run');
    expect(run.events).toHaveLength(1); // result merged, not its own item
    expect(run.events[0].result).toEqual({ isError: true });
  });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run src/components/replay/stream/__tests__/streamModel.test.ts`. Expected: FAIL.

- [ ] **Step 3: 구현** — `streamModel.ts`:

```ts
import type { ObservedEventDto } from '../../../api/types';

export type StreamRole = 'user' | 'assistant' | 'thinking';
export interface MessageItem { type: 'message'; id: string; eventId: string; role: StreamRole; model: string | null; text: string; timestamp: string; }
export interface ActivityEvent { event: ObservedEventDto; result: { isError: boolean } | null; }
export interface ActivityRun { type: 'activity-run'; id: string; events: ActivityEvent[]; }
export type StreamItem = MessageItem | ActivityRun;

const SCAFFOLD = /^\s*(<command-name>|<command-message>|<command-args>|<local-command-stdout>|<local-command-caveat>|Base directory for this skill:|\[Request interrupted)/;
function asObj(v: unknown): Record<string, unknown> { return v && typeof v === 'object' ? v as Record<string, unknown> : {}; }
function userText(p: Record<string, unknown>): string { return (typeof p.content === 'string' ? p.content : typeof p.text === 'string' ? p.text : '').trim(); }

// classify → 'message' | 'activity' | 'drop'
function classify(e: ObservedEventDto): { cat: 'message' | 'activity' | 'drop'; role?: StreamRole; text?: string; model?: string | null } {
  const p = asObj(e.payload);
  if (e.kind === 'user_message') {
    const t = userText(p);
    if (t === '') return { cat: 'drop' };          // empty → dropped (still listed in run for count? we DROP from stream entirely per spec; see note)
    if (SCAFFOLD.test(t)) return { cat: 'activity' }; // scaffolding → absorbed
    return { cat: 'message', role: 'user', text: t, model: null };
  }
  if (e.kind === 'assistant_message') {
    const t = (typeof p.text === 'string' ? p.text : '').trim();
    return t ? { cat: 'message', role: 'assistant', text: t, model: (p.model as string) ?? null } : { cat: 'drop' };
  }
  if (e.kind === 'thinking') {
    const t = (typeof p.thinking === 'string' ? p.thinking : '').trim();
    return t ? { cat: 'message', role: 'thinking', text: t, model: null } : { cat: 'activity' }; // redacted → activity
  }
  if (e.kind === 'system_summary') return { cat: 'drop' };
  return { cat: 'activity' }; // tool_call, hook_event, session_state, metric_sample, otel_span, log_record, attachment_meta, tool_result
}

export function buildStreamModel(events: ObservedEventDto[]): StreamItem[] {
  // index tool_result by tool_use_id for merge
  const resultByUse = new Map<string, ObservedEventDto>();
  for (const e of events) if (e.kind === 'tool_result' && e.tool_use_id) resultByUse.set(e.tool_use_id, e);

  const items: StreamItem[] = [];
  let run: ActivityEvent[] = [];
  const flush = () => { if (run.length) { items.push({ type: 'activity-run', id: `run-${run[0].event.event_id}`, events: run }); run = []; } };

  for (const e of events) {
    if (e.kind === 'tool_result') continue; // merged into its tool_call
    const c = classify(e);
    if (c.cat === 'message') {
      flush();
      items.push({ type: 'message', id: e.event_id, eventId: e.event_id, role: c.role!, model: c.model ?? null, text: c.text!, timestamp: e.observed_at });
    } else if (c.cat === 'activity') {
      let result: { isError: boolean } | null = null;
      if (e.kind === 'tool_call' && e.tool_use_id) {
        const r = resultByUse.get(e.tool_use_id);
        if (r) result = { isError: asObj(asObj(r.payload).tool_result).is_error === true };
      }
      run.push({ event: e, result });
    }
    // 'drop' → nothing
  }
  flush();
  return items;
}
```

> NOTE: empty user_message는 spec상 "제외". scaffolding은 "축약 흡수". 빈 카드가 run에 들어가면 count만 부풀리므로 empty는 drop. 이 결정을 테스트가 잠근다(위 케이스: empty+scaffolding → message 0, activity-run 존재).

- [ ] **Step 4: 통과 확인** — `npx vitest run src/components/replay/stream/__tests__/streamModel.test.ts`. Expected: PASS.

- [ ] **Step 5: commit** — `git add webui/src/components/replay/stream/streamModel.ts webui/src/components/replay/stream/__tests__/streamModel.test.ts && git commit -m "webui(stream): buildStreamModel classifier — message/activity/drop + content|text bug fix"`

---

## S3 — 프론트: activity 그룹핑(phase 분할) (TDD)

`ActivityRun`을 episode phase 경계로 1~2개 `ActivityStackData`로 쪼개고 요약 집계.

### Task S3.1: phase 분할 + 요약 집계

**Files:** Create `webui/src/components/replay/stream/activityGroup.ts` + `__tests__/activityGroup.test.ts`.

- [ ] **Step 1: 실패 테스트** — `activityGroup.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { splitRunByPhase, summarizeStack } from '../activityGroup';
import type { ActivityEvent } from '../streamModel';

const a = (id: string, kind: string, tool?: string, isErr?: boolean): ActivityEvent => ({
  event: { event_id: id, kind, observed_at: `2026-05-28T00:00:0${id}Z`, tool_name: tool ?? null,
    payload: tool ? { tool_name: tool, input: {} } : {} } as any,
  result: isErr === undefined ? null : { isError: isErr },
});

const phaseOf = (eid: string): string | null => ({ '1': 'exploration', '2': 'exploration', '3': 'action', '4': 'action' } as any)[eid] ?? null;

describe('splitRunByPhase', () => {
  it('splits a run at phase boundaries (max 2 stacks)', () => {
    const run = [a('1','tool_call','Read'), a('2','tool_call','Read'), a('3','tool_call','Bash'), a('4','hook_event')];
    const stacks = splitRunByPhase(run, phaseOf);
    expect(stacks.map((s) => s.phase)).toEqual(['exploration', 'action']);
    expect(stacks[0].events.map((e) => e.event.event_id)).toEqual(['1', '2']);
    expect(stacks[1].events.map((e) => e.event.event_id)).toEqual(['3', '4']);
  });
  it('single phase → one stack', () => {
    const run = [a('1','tool_call','Read'), a('2','tool_call','Read')];
    expect(splitRunByPhase(run, phaseOf)).toHaveLength(1);
  });
  it('caps at 2 stacks: a third phase merges into the last', () => {
    const run = [a('1','tool_call','Read'), a('3','tool_call','Bash'),
      { event: { event_id: '9', kind: 'tool_call', observed_at: 'z', tool_name: 'Edit', payload: {} } as any, result: null }]; // phase null
    const stacks = splitRunByPhase(run, phaseOf);
    expect(stacks.length).toBeLessThanOrEqual(2);
    expect(stacks.at(-1)!.events.map((e) => e.event.event_id)).toContain('9');
  });
});

describe('summarizeStack', () => {
  it('aggregates top tools with ×N, error count, total + duration', () => {
    const s = summarizeStack({ phase: 'exploration', events: [
      a('1','tool_call','Read'), a('2','tool_call','Read'), a('3','tool_call','Bash', true)] });
    expect(s.count).toBe(3);
    expect(s.topTools).toEqual(['Read ×2', 'Bash']);
    expect(s.errorCount).toBe(1);
  });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run src/components/replay/stream/__tests__/activityGroup.test.ts`. Expected: FAIL.

- [ ] **Step 3: 구현** — `activityGroup.ts`:

```ts
import type { ActivityEvent } from './streamModel';

export interface ActivityStackData { phase: string | null; events: ActivityEvent[]; }
export interface StackSummary { phase: string | null; count: number; topTools: string[]; errorCount: number; durationMs: number; }

export function splitRunByPhase(run: ActivityEvent[], phaseOf: (eventId: string) => string | null): ActivityStackData[] {
  const stacks: ActivityStackData[] = [];
  for (const ae of run) {
    const ph = phaseOf(ae.event.event_id);
    const last = stacks.at(-1);
    if (last && (last.phase === ph || stacks.length >= 2)) last.events.push(ae);
    else stacks.push({ phase: ph, events: [ae] });
  }
  return stacks;
}

export function summarizeStack(stack: ActivityStackData): StackSummary {
  const counts = new Map<string, number>();
  let errorCount = 0;
  let min = Infinity, max = -Infinity;
  for (const { event, result } of stack.events) {
    const name = event.tool_name ?? event.kind;
    counts.set(name, (counts.get(name) ?? 0) + 1);
    if (result?.isError) errorCount++;
    const t = new Date(event.observed_at).getTime();
    if (!Number.isNaN(t)) { min = Math.min(min, t); max = Math.max(max, t); }
  }
  const topTools = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 2)
    .map(([n, c]) => (c > 1 ? `${n} ×${c}` : n));
  return { phase: stack.phase, count: stack.events.length, topTools, errorCount, durationMs: max >= min ? max - min : 0 };
}
```

> phase 분할 규칙은 이 한 파일에 격리 — spec 열린질문(분할 vs 런당1개) 전환 비용 최소화.

- [ ] **Step 4: 통과 확인** — `npx vitest run src/components/replay/stream/__tests__/activityGroup.test.ts`. Expected: PASS.

- [ ] **Step 5: commit** — `git add webui/src/components/replay/stream/activityGroup.ts webui/src/components/replay/stream/__tests__/activityGroup.test.ts && git commit -m "webui(stream): activity run phase-split + stack summary"`

---

## S5 — 프론트: 채팅 레이아웃 + activity 스택 컴포넌트 (TDD)

### Task S5.1: ActivityStack 컴포넌트

**Files:** Create `webui/src/components/replay/stream/ActivityStack.tsx` + `ActivityStack.module.css` + `__tests__/ActivityStack.test.tsx`.

- [ ] **Step 1: 실패 테스트** — `ActivityStack.test.tsx`:

```tsx
import { render, screen, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { ActivityStack } from '../ActivityStack';
import type { ActivityStackData } from '../activityGroup';

const stack: ActivityStackData = { phase: 'exploration', events: [
  { event: { event_id: 'c1', kind: 'tool_call', observed_at: 'z', tool_name: 'Read', payload: { tool_name: 'Read', input: { file_path: '/a/x.jpg' } } } as any, result: { isError: false } },
  { event: { event_id: 'c2', kind: 'tool_call', observed_at: 'z', tool_name: 'Bash', payload: { tool_name: 'Bash', input: { command: 'ls' } } } as any, result: { isError: true } },
] };

describe('ActivityStack', () => {
  it('renders a collapsed summary with phase, top tools, count, error badge', () => {
    render(<ActivityStack stack={stack} selectedEventId={null} onSelect={() => {}} />);
    const s = screen.getByTestId('activity-stack');
    expect(s).toHaveAttribute('data-phase', 'exploration');
    expect(s).toHaveAttribute('data-count', '2');
    expect(s).toHaveAttribute('data-errors', '1');
    expect(within(s).getByText(/Read/)).toBeInTheDocument();
    // collapsed: item list not shown
    expect(screen.queryByTestId('activity-item')).toBeNull();
  });

  it('expands on click to show items, each selectable', () => {
    const onSelect = vi.fn();
    render(<ActivityStack stack={stack} selectedEventId={null} onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId('activity-stack-toggle'));
    const items = screen.getAllByTestId('activity-item');
    expect(items).toHaveLength(2);
    expect(within(items[0]).getByText('Read')).toBeInTheDocument();
    fireEvent.click(items[1]);
    expect(onSelect).toHaveBeenCalledWith('c2');
  });

  it('marks the selected item', () => {
    render(<ActivityStack stack={stack} selectedEventId="c1" onSelect={() => {}} />);
    fireEvent.click(screen.getByTestId('activity-stack-toggle'));
    const items = screen.getAllByTestId('activity-item');
    expect(items[0].getAttribute('data-selected')).toBe('true');
  });
});
```

- [ ] **Step 2: 실패 확인** — `npx vitest run .../ActivityStack.test.tsx`. Expected: FAIL.

- [ ] **Step 3: 구현** — `ActivityStack.tsx` (lucide `Wrench`/`ChevronRight`, `summarizeStack`·`nodeLabel` 사용). data 속성: `data-testid="activity-stack" data-phase data-count data-errors`, toggle 버튼 `data-testid="activity-stack-toggle"`, 항목 `data-testid="activity-item" data-selected`. 펼침 state는 `useState(false)`. 각 항목 label = `nodeLabel({node_kind: e.event.kind, payload: e.event.payload})`의 primary/secondary, ok/error 배지. 스타일은 brainstorm mockup(`activity-stack.html`) 참고, witmcc 토큰 사용(--witmcc-surface-2 #161a23, --witmcc-border #1d212c, phase 색 --witmcc-phase-*).

- [ ] **Step 4: 통과 확인 + Step 5: commit** — `git commit -m "webui(stream): ActivityStack — collapsed phase summary, expand, item select"`

### Task S5.2: 채팅 버블 (MessageCard) + 역할 라벨

**Files:** Rewrite `webui/src/components/replay/stream/StreamCard.tsx` → message bubble; `StreamCard.module.css`; update `__tests__/StreamCard.test.tsx`.

- [ ] **Step 1: 실패 테스트** — `StreamCard.test.tsx` (재작성):

```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MessageCard } from '../StreamCard';
import type { MessageItem } from '../streamModel';

const m = (over: Partial<MessageItem>): MessageItem => ({ type: 'message', id: 'x', eventId: 'x', role: 'user', model: null, text: 'hi', timestamp: '2026-05-28T09:14:02Z', ...over });

describe('MessageCard', () => {
  it('user message aligns right with You label', () => {
    render(<MessageCard item={m({ role: 'user', text: '질문' })} selected={false} onSelect={() => {}} />);
    const c = screen.getByTestId('message-card');
    expect(c).toHaveAttribute('data-role', 'user');
    expect(c).toHaveAttribute('data-align', 'right');
    expect(screen.getByText('You')).toBeInTheDocument();
    expect(screen.getByText('질문')).toBeInTheDocument();
  });
  it('assistant message aligns left with model name', () => {
    render(<MessageCard item={m({ role: 'assistant', model: 'claude-opus-4-8', text: '답변' })} selected={false} onSelect={() => {}} />);
    const c = screen.getByTestId('message-card');
    expect(c).toHaveAttribute('data-role', 'assistant');
    expect(c).toHaveAttribute('data-align', 'left');
    expect(screen.getByText('Opus 4.8')).toBeInTheDocument();
  });
  it('thinking is left + distinct (data-role=thinking)', () => {
    render(<MessageCard item={m({ role: 'thinking', text: '추론중' })} selected={false} onSelect={() => {}} />);
    expect(screen.getByTestId('message-card')).toHaveAttribute('data-role', 'thinking');
  });
  it('forwards click with eventId', () => {
    const onSelect = vi.fn();
    render(<MessageCard item={m({ eventId: 'e1' })} selected={false} onSelect={onSelect} />);
    screen.getByTestId('message-card').click();
    expect(onSelect).toHaveBeenCalledWith('e1');
  });
});
```

- [ ] **Step 2: 실패 확인.**
- [ ] **Step 3: 구현** — `MessageCard`: `data-testid="message-card" data-role={role} data-align={role==='user'?'right':'left'} data-selected`. 헤더: 아바타(lucide `User`/`Bot`/`BrainCog`) + 라벨(user→`You`, assistant→`formatModel(model)`, thinking→`추론`). 본문: text. thinking은 점선 좌측 테두리·이탤릭·muted. user는 우측 정렬 accent 버블. `formatModel`은 `nodeLabel.ts`에서 import.
- [ ] **Step 4: 통과 + Step 5: commit** — `git commit -m "webui(stream): MessageCard chat bubbles — role align, avatar, model label"`

### Task S5.3: ConversationStream에 새 모델 배선

**Files:** Modify `ConversationStream.tsx` + `__tests__/ConversationStream.test.tsx`.

- [ ] **Step 1: 실패 테스트** — props를 `items: StreamItem[]`로 바꾸고, message는 MessageCard·activity-run은 phase 분할 후 ActivityStack 렌더. 핵심 테스트: items 혼합 입력 → message-card N개 + activity-stack M개 순서대로 렌더; selectedEventId가 message면 그 카드, activity 항목이면 그 스택이 펼쳐지고 항목 선택. 기존 가상화·scroll-into-view·autoscroll·FALLBACK_CAP 테스트는 message-card 기준으로 유지.

```tsx
it('renders messages and activity stacks in order', () => {
  const items = [ /* message, activity-run(2 events), message */ ];
  render(<ConversationStream items={items} phaseOf={() => null} selectedEventId={null} onSelect={() => {}} />);
  expect(screen.getAllByTestId('message-card')).toHaveLength(2);
  expect(screen.getAllByTestId('activity-stack')).toHaveLength(1);
});
```

> props 변경: `cards`→`items`, `phaseByEventId`→`phaseOf` 함수(activity 분할용), `findingEventIds`는 message/activity 마커용 유지. scroll-into-view는 `data-event-id`를 message-card·activity-stack 양쪽에 부여해 유지.

- [ ] **Step 2~4:** 구현 + 기존 가상화/autoscroll/scroll-into-view/FALLBACK_CAP 테스트가 새 구조에서 통과하도록 갱신. activity-run은 `splitRunByPhase(run.events, phaseOf)`로 스택들 렌더.
- [ ] **Step 5: commit** — `git commit -m "webui(stream): ConversationStream renders StreamItem[] (messages + activity stacks)"`

---

## S6 — 프론트: 우측 패널 2탭 재구성 (TDD)

### Task S6.1: NodeDetail 컴포넌트 (종류별 상세)

**Files:** Create `webui/src/components/replay/detail/NodeDetail.tsx` + `__tests__/NodeDetail.test.tsx`.

- [ ] **Step 1: 실패 테스트** — `NodeDetail.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { NodeDetail } from '../NodeDetail';

const node = (kind: string, payload: unknown, id = 'nd_1') => ({ node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: '2026-05-15T05:25:39Z', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload }) as any;

describe('NodeDetail', () => {
  it('tool_call: shows tool name + parameters + result', () => {
    render(<NodeDetail node={node('tool_call', { tool_name: 'Bash', input: { command: 'rm -f x', description: '정리' } })}
      record={{ tool_result: { is_error: false } }} episodePhase="action" findings={[]} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
    expect(screen.getByText('rm -f x')).toBeInTheDocument();   // command param
    expect(screen.getByText('정리')).toBeInTheDocument();       // description param
    expect(screen.getByText(/ok/i)).toBeInTheDocument();        // result badge
  });
  it('assistant_message: shows full text + token usage from record', () => {
    render(<NodeDetail node={node('assistant_message', { model: 'claude-opus-4-8', text: '전체 답변' })}
      record={{ message: { usage: { output_tokens: 451, input_tokens: 3 } } }} episodePhase={null} findings={[]} />);
    expect(screen.getByText('전체 답변')).toBeInTheDocument();
    expect(screen.getByText(/451/)).toBeInTheDocument();
  });
  it('renders findings for the node', () => {
    render(<NodeDetail node={node('tool_call', { tool_name: 'Read', input: {} })} record={null} episodePhase={null}
      findings={[{ finding_id: 'f1', severity: 'medium', category: 'missing_verification', confidence: 0.8, summary: '검증 없음' } as any]} />);
    expect(screen.getByText('missing_verification')).toBeInTheDocument();
    expect(screen.getByText(/검증 없음/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 실패 확인.**
- [ ] **Step 3: 구현** — `NodeDetail.tsx`: 헤더(nodeLabel 아이콘+primary, id 작게), 공통(시각·소요·episode), 종류별 섹션. tool_call: payload.input의 key-value(긴 값/command는 `<pre>` 코드블록, file path mono) + 결과 배지(record.tool_result.is_error). assistant_message: payload.text 전체 + 토큰(record.message.usage — DetailTab 로직 재사용). hook_event: hookName/exitCode/stdout. otel_span: name/duration. findings 리스트(InsightTab의 finding 마크업 재사용). DetailTab의 `fmtTime`·token 배지 로직을 여기로 이전.
- [ ] **Step 4: 통과 + Step 5: commit** — `git commit -m "webui(detail): NodeDetail — per-kind focused node detail (tool params, tokens, findings)"`

### Task S6.2: InsightTab = subgraph + NodeDetail, DetailPanel 2탭

**Files:** Modify `InsightTab.tsx` (+test), `DetailPanel.tsx` (+test); delete `DetailTab.tsx` + its test.

- [ ] **Step 1: 실패 테스트** — `DetailPanel.test.tsx` 갱신:

```tsx
it('shows only Insight and Raw tabs (Detail tab removed)', () => {
  render(<DetailPanel node={someNode} record={null} findings={[]} episodePhase={null} nodes={[someNode]} edges={[]} onSelectNode={() => {}} />);
  expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
  expect(screen.queryByRole('tab', { name: /^detail$/i })).toBeNull();
});
it('Insight tab contains the focused subgraph AND node detail', () => {
  render(<DetailPanel node={toolNode} record={{tool_result:{is_error:false}}} findings={[]} episodePhase="action" nodes={[toolNode]} edges={[]} onSelectNode={() => {}} />);
  expect(screen.getByTestId('focused-graph') ?? screen.getByText(/select a node/i)).toBeTruthy();
  expect(screen.getByText(/Bash|Read/)).toBeInTheDocument(); // node detail label present
});
```
그리고 `InsightTab.test.tsx`에 NodeDetail 포함·findings가 NodeDetail로 이동했는지 단언.

- [ ] **Step 2: 실패 확인.**
- [ ] **Step 3: 구현** — `InsightTab`에 `node`/`record`/`episodePhase` props 추가, `<FocusedInsightGraph/>` 아래 `<NodeDetail .../>` 렌더(findings를 NodeDetail로 위임). `DetailPanel`: TabId를 `'insight'|'raw'`로, `tab('detail',...)` 제거, fallback 항상 `'insight'`, InsightTab에 node/record/episodePhase 전달. `DetailTab.tsx` + 테스트 삭제.
- [ ] **Step 4: 통과 확인** — `npx vitest run src/components/replay/detail`. Expected: PASS.
- [ ] **Step 5: commit** — `git add -A webui/src/components/replay/detail && git commit -m "webui(detail): 2-tab panel — Insight(subgraph+NodeDetail+findings) / Raw; drop Detail tab"`

### Task S6.3: SessionDetailPage 배선 갱신

**Files:** Modify `webui/src/routes/SessionDetailPage.tsx` + `__tests__/SessionDetailPage.test.tsx`.

- [ ] **Step 1:** `buildStreamCards`→`buildStreamModel(window_.events)`로 교체, `streamItems` memo. `phaseOf(eventId)` 함수 memo(기존 `phaseByEventId` 로직을 event_id→phase 함수로). `ConversationStream`에 `items`/`phaseOf` 전달. `DetailPanel`에서 Detail 탭 관련 prop 정리(node/record/findings/episodePhase/nodes/edges/onSelectNode 유지). selectStreamCard는 message eventId·activity 항목 eventId 모두 처리(nodeIdByEventId 매핑 유지). 기존 cross-sync 통합 테스트(SessionDetailPage.test.tsx의 'cross-syncs…')가 새 구조에서 통과하도록 fixture/셀렉터 갱신(message-card[data-event-id], 또는 activity 항목).
- [ ] **Step 2: 실패 확인 → Step 3: 구현 → Step 4: 통과.** 기존 'clicking a node shows the DetailPanel tablist'·cross-sync·R1 layout 테스트 갱신.
- [ ] **Step 5: commit** — `git commit -m "webui(stream): wire SessionDetailPage to buildStreamModel + phaseOf + 2-tab panel"`

### Task S6.4: 전체 프론트 회귀 + 타입/빌드

- [ ] **Step 1:** `cd webui && npx vitest run && npx tsc --noEmit && npm run build`. 깨진 기존 테스트(streamModel/StreamCard/ConversationStream 사용처) 모두 새 API로 갱신. Expected: all green, tsc rc=0, build ok.
- [ ] **Step 2: commit**(필요 시) — `git commit -m "webui: fix fallout from stream/panel rewrite; suite green"`

---

## S7 — 스모크 + PR

### Task S7.1: 백엔드 빌드 + re-ingest + 브라우저 스모크

- [ ] **Step 1:** `cargo build`. dev DB 재생성 필요: `./target/debug/witmcc init-db` 후 `./target/debug/witmcc ingest --all`(payload enrichment 반영). (실데이터 transcript 경로는 기존 설정 사용.)
- [ ] **Step 2:** `./target/debug/witmcc serve --port 7878` + claude-in-chrome으로 실세션(findings·tool 많은 세션, 예 `0056c8f5…` 또는 `0f1e71f6…`) 검증:
  - 스트림: user 우측 / assistant 좌측(모델명) / 추론 구분 / **빈 카드 사라짐** / scaffolding·redacted가 activity 스택으로 흡수.
  - activity 스택: phase 분할(1~2), 요약(대표 도구·×N·에러), 펼침 목록, 항목 클릭 → 우측 선택.
  - 노드 라벨: timeline 툴팁·subgraph가 "Read · file" 류로 표기(해시 id 아님).
  - 우측: 탭 2개(Insight/Raw), Insight 하단에 포커스 노드 상세(tool 파라미터·결과·토큰)·finding.
  - cross-sync·scroll·줌 등 기존 동작 유지. 콘솔 에러 0.
- [ ] **Step 3:** 발견 이슈 수정(해당 task 재진입). 스크린샷 2~3장 캡처.

### Task S7.2: 최종 검토 + PR

- [ ] **Step 1:** 전체 스위트 카운트 기록, `docs/implementation-notes.html` 갱신(스트림 재설계 결정·편차: 백엔드 label 필드 대신 프론트 nodeLabel 단일 deriver 채택, phase-split 그룹핑).
- [ ] **Step 2:** `gh pr create --base main` — 본문에 조사 결과(7,971 빈 카드 버그)·설계·슬라이스·스모크·테스트 카운트. (브랜치는 현재 `webui/redesign-v2-replay-layout` 위에 쌓거나, 신규 브랜치 — 실행 시 결정.)

---

## Self-Review

**Spec coverage:** §1 버그(content|text) → S2.1 테스트. §1 노이즈 → S2 scaffolding/empty 분류. §2 분류(1급/축약/제외) → S2. §3 채팅 레이아웃·역할 라벨·모델명 → S5.2 + S1.2(model). §4 phase 분할 activity → S3 + S5.1. §5 2탭 패널·NodeDetail·tool 파라미터 → S6. §6 노드 라벨·tool_name/model surfacing → S1 + S4. §7 실데이터 앵커링 → S1/S4 fixture·실값 테스트. §8 테스트 → 각 task TDD. §9 non-goals 준수(파생 레이어, 원시 보존). §10 열린질문(phase 분할 격리 S3, 모델 표기 formatModel).

**편차(의도적):** spec §6의 "백엔드 label 필드"를 채택하지 않고 **프론트 `nodeLabel` 단일 deriver + 백엔드 payload enrichment(tool_name/model)**로 동일 결과를 더 적은 코드로 달성(spec의 fallback-derivation 허용 범위). DRY·serve-time 비용 0. implementation-notes에 기록(S7.2).

**Placeholder scan:** 모든 코드 step에 실제 테스트/구현 코드 포함. S5.3/S6.3은 기존 대형 컴포넌트라 핵심 테스트+구조+data 속성 명시(구현자는 기존 패턴 따름). "기존 테스트 갱신"은 구체 대상(streamModel/StreamCard/ConversationStream/SessionDetailPage 사용처) 명시.

**Type consistency:** `StreamItem`=`MessageItem`|`ActivityRun`(S2), `ActivityEvent`(S2)→`splitRunByPhase`/`summarizeStack`(S3)→`ActivityStack`(S5.1). `MessageItem`→`MessageCard`(S5.2)→`ConversationStream items`(S5.3)→`SessionDetailPage`(S6.3). `nodeLabel`/`formatModel`(S4)→S5.1/S5.2/S6.1. `NodeDetail` props(node/record/episodePhase/findings)(S6.1)→InsightTab(S6.2). 일관.

**열린 위험:** hook payload 두 형태(top-level hookName vs `{hook:{hook_event_name}}`)는 nodeLabel이 둘 다 읽음(S4 테스트 양쪽). re-ingest 후 실데이터에서 hook/otel 라벨 스모크 확인(S7.1).
