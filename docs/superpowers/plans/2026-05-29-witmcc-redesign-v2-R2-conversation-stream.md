# Redesign v2 — R2: Conversation Stream — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Fill the `stream` slot with a chat-style conversation stream — User / Thinking / Assistant / Tool cards, oldest→newest with newest at the bottom, scrollable and virtualized, clicking a card selects the corresponding graph node.

**Architecture:** A pure mapper turns the windowed `ObservedEventDto[]` into a `StreamCard[]` view model (one card per user_message / assistant_message / thinking / tool_call; tool_result is merged into its tool_call by `tool_use_id`). `StreamCard` renders a card variant with lucide-react icons and the inline fields. `ConversationStream` lists the cards inside a virtualizer, auto-scrolls to the bottom on new data unless the user scrolled up, and reports clicks to the existing `ReplaySelection`. Real-data note: the backend already normalizes tool results into their own `tool_result` kind (actor=system), so there is NO "tool_result-only user message" to fold — `user_message` is always a genuine prompt. Token usage lives only in the raw record (`/v1/events/:id/raw`), so the token badge is deferred to R3's Detail tab, not rendered inline (avoids one raw fetch per card).

**Tech Stack:** React 18, TypeScript, `lucide-react` (new), `@tanstack/react-virtual` (new), CSS Modules, Vitest + Testing Library (jsdom).

**Spec:** `docs/superpowers/specs/2026-05-29-witmcc-ux-redesign-v2-design.md` §3. Resolves feedback #4 (chat-like reading).

**Real-data anchor (observed from live API on session e301d123, 2026-05-29):**
- `user_message` — `actor:'user'`, `payload:{ content: string }`.
- `assistant_message` — `actor:'assistant'`, `payload:{ content_ordinal, text }`.
- `thinking` — `actor:'assistant'`, `payload:{ content_ordinal, thinking, signature }` (`thinking` may be empty string when redacted).
- `tool_call` — `actor:'assistant'`, `tool_name`, `tool_use_id`, `payload:{ content_ordinal, input: object }`.
- `tool_result` — `actor:'system'`, `tool_use_id`, `payload:{ content_ordinal, tool_result:{ type, tool_use_id, content, is_error? } }` (`is_error` present only when erroring).

---

## File Structure

- **Create** `webui/src/components/replay/stream/streamModel.ts` — pure `buildStreamCards(events)` mapper + types. One responsibility: event→card view model.
- **Create** `webui/src/components/replay/stream/__tests__/streamModel.test.ts`.
- **Create** `webui/src/components/replay/stream/StreamCard.tsx` + `StreamCard.module.css` — one card, all variants.
- **Create** `webui/src/components/replay/stream/__tests__/StreamCard.test.tsx`.
- **Create** `webui/src/components/replay/stream/ConversationStream.tsx` + `ConversationStream.module.css` — virtualized list + autoscroll.
- **Create** `webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx`.
- **Modify** `webui/src/routes/SessionDetailPage.tsx` — replace the `stream` slot placeholder `<p>` with `<ConversationStream>`, passing `window_.events`, the graph (for event→node map + finding markers), episodes, findings, and selection.
- **Modify** `webui/package.json` — add `lucide-react`, `@tanstack/react-virtual`.

---

### Task 1: Add dependencies

**Files:** Modify `webui/package.json` (+ lockfile).

- [ ] **Step 1: Install**

Run: `cd webui && npm install lucide-react @tanstack/react-virtual`
Expected: both added to `dependencies`, lockfile updated.

- [ ] **Step 2: Sanity build**

Run: `cd webui && npx tsc --noEmit && npm run build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add webui/package.json webui/package-lock.json
git commit -m "webui(redesign-v2) R2: add lucide-react + react-virtual deps"
```

---

### Task 2: Stream card view-model mapper (TDD)

**Files:**
- Create: `webui/src/components/replay/stream/streamModel.ts`
- Test: `webui/src/components/replay/stream/__tests__/streamModel.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// webui/src/components/replay/stream/__tests__/streamModel.test.ts
/**
 * R2 RED — buildStreamCards maps normalized ObservedEventDto[] into the chat
 * view model. Shapes anchored to the live API (plan R2 real-data anchor).
 */
import { describe, expect, it } from 'vitest';
import { buildStreamCards } from '../streamModel';
import type { ObservedEventDto } from '../../../../api/types';

function ev(partial: Partial<ObservedEventDto>): ObservedEventDto {
  return {
    event_id: 'e', raw_event_id: '', session_id: 's', event_uuid: null,
    parent_uuid: null, observed_at: '2026-05-28T00:00:00Z', actor: 'user',
    kind: 'user_message', subkind: null, tool_use_id: null, tool_name: null,
    turn_id: null, is_sidechain: false, is_meta: false, payload: {},
    ...partial,
  };
}

describe('buildStreamCards', () => {
  it('keeps only conversation kinds, dropping session_state/hook/otel/etc', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'u', kind: 'user_message', actor: 'user', payload: { content: 'hi' } }),
      ev({ event_id: 'noise', kind: 'session_state', actor: 'system', payload: { permissionMode: 'x' } }),
      ev({ event_id: 'm', kind: 'metric_sample', actor: 'system', payload: {} }),
    ]);
    expect(cards.map((c) => c.id)).toEqual(['u']);
  });

  it('maps user_message to a user card with the prompt text', () => {
    const [c] = buildStreamCards([ev({ event_id: 'u', kind: 'user_message', actor: 'user', payload: { content: 'fix build.rs' } })]);
    expect(c.kind).toBe('user');
    expect(c.preview).toBe('fix build.rs');
  });

  it('maps assistant_message text and thinking to their own cards', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'a', kind: 'assistant_message', actor: 'assistant', payload: { content_ordinal: 0, text: 'on it' } }),
      ev({ event_id: 't', kind: 'thinking', actor: 'assistant', payload: { content_ordinal: 0, thinking: 'reasoning…', signature: 'x' } }),
    ]);
    expect(cards.find((c) => c.id === 'a')?.kind).toBe('assistant');
    expect(cards.find((c) => c.id === 'a')?.preview).toBe('on it');
    expect(cards.find((c) => c.id === 't')?.kind).toBe('thinking');
    expect(cards.find((c) => c.id === 't')?.preview).toBe('reasoning…');
  });

  it('represents empty/redacted thinking with a placeholder preview', () => {
    const [c] = buildStreamCards([ev({ event_id: 't', kind: 'thinking', actor: 'assistant', payload: { content_ordinal: 0, thinking: '', signature: 'x' } })]);
    expect(c.kind).toBe('thinking');
    expect(c.preview).toMatch(/redacted|hidden/i);
  });

  it('merges tool_result into its tool_call by tool_use_id (no standalone result card)', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'tc', kind: 'tool_call', actor: 'assistant', tool_name: 'Bash', tool_use_id: 'tu1', payload: { input: { command: 'cargo test', description: 'run', timeout: 60 } } }),
      ev({ event_id: 'tr', kind: 'tool_result', actor: 'system', tool_use_id: 'tu1', payload: { tool_result: { type: 'tool_result', tool_use_id: 'tu1', content: 'ok' } } }),
    ]);
    expect(cards).toHaveLength(1);
    const c = cards[0];
    expect(c.kind).toBe('tool');
    expect(c.id).toBe('tc');
    expect(c.tool?.toolName).toBe('Bash');
    expect(c.tool?.inputSummary).toBe('cargo test');
    expect(c.tool?.result?.isError).toBe(false);
  });

  it('flags is_error on the merged tool result', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'tc', kind: 'tool_call', actor: 'assistant', tool_name: 'Edit', tool_use_id: 'tu2', payload: { input: { file_path: 'src/graph/build.rs' } } }),
      ev({ event_id: 'tr', kind: 'tool_result', actor: 'system', tool_use_id: 'tu2', payload: { tool_result: { type: 'tool_result', tool_use_id: 'tu2', content: 'boom', is_error: true } } }),
    ]);
    expect(cards[0].tool?.inputSummary).toBe('src/graph/build.rs');
    expect(cards[0].tool?.result?.isError).toBe(true);
  });

  it('keeps a tool_call with no matching result (result null)', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'tc', kind: 'tool_call', actor: 'assistant', tool_name: 'Read', tool_use_id: 'tu3', payload: { input: { file_path: 'a.ts' } } }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].tool?.result == null).toBe(true);
  });
});
```

- [ ] **Step 2: Run, verify fail**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/streamModel.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```ts
// webui/src/components/replay/stream/streamModel.ts
import type { ObservedEventDto } from '../../../api/types';

export type StreamCardKind = 'user' | 'assistant' | 'thinking' | 'tool';

export interface ToolResultView {
  isError: boolean;
  preview: string;
}

export interface ToolCardView {
  toolName: string | null;
  toolUseId: string | null;
  inputSummary: string;
  result: ToolResultView | null;
}

export interface StreamCard {
  id: string;
  kind: StreamCardKind;
  actor: string;
  timestamp: string;
  preview: string;
  tool: ToolCardView | null;
  /** Source event id, for graph-node correlation in the host. */
  eventId: string;
}

const CONVERSATION_KINDS = new Set(['user_message', 'assistant_message', 'thinking', 'tool_call']);

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function toolInputSummary(toolName: string | null, input: unknown): string {
  const i = asObj(input);
  if (typeof i.command === 'string') return i.command;
  if (typeof i.file_path === 'string') return i.file_path;
  if (typeof i.pattern === 'string') return i.pattern;
  if (typeof i.skill === 'string') return i.skill;
  const keys = Object.keys(i);
  if (keys.length === 0) return toolName ?? '';
  try {
    return JSON.stringify(i);
  } catch {
    return keys.join(', ');
  }
}

function resultPreview(content: unknown): string {
  if (typeof content === 'string') return content;
  try {
    return JSON.stringify(content);
  } catch {
    return '';
  }
}

export function buildStreamCards(events: ObservedEventDto[]): StreamCard[] {
  // Index tool_result events by tool_use_id so we can merge them into calls.
  const resultsByToolUseId = new Map<string, ObservedEventDto>();
  for (const e of events) {
    if (e.kind === 'tool_result' && e.tool_use_id) {
      resultsByToolUseId.set(e.tool_use_id, e);
    }
  }

  const cards: StreamCard[] = [];
  for (const e of events) {
    if (!CONVERSATION_KINDS.has(e.kind)) continue;
    const p = asObj(e.payload);

    if (e.kind === 'user_message') {
      cards.push({ id: e.event_id, eventId: e.event_id, kind: 'user', actor: e.actor, timestamp: e.observed_at, preview: typeof p.content === 'string' ? p.content : '', tool: null });
    } else if (e.kind === 'assistant_message') {
      cards.push({ id: e.event_id, eventId: e.event_id, kind: 'assistant', actor: e.actor, timestamp: e.observed_at, preview: typeof p.text === 'string' ? p.text : '', tool: null });
    } else if (e.kind === 'thinking') {
      const t = typeof p.thinking === 'string' ? p.thinking : '';
      cards.push({ id: e.event_id, eventId: e.event_id, kind: 'thinking', actor: e.actor, timestamp: e.observed_at, preview: t.trim() === '' ? '(thinking redacted)' : t, tool: null });
    } else if (e.kind === 'tool_call') {
      const resultEv = e.tool_use_id ? resultsByToolUseId.get(e.tool_use_id) : undefined;
      let result: ToolResultView | null = null;
      if (resultEv) {
        const tr = asObj(asObj(resultEv.payload).tool_result);
        result = { isError: tr.is_error === true, preview: resultPreview(tr.content) };
      }
      cards.push({
        id: e.event_id, eventId: e.event_id, kind: 'tool', actor: e.actor, timestamp: e.observed_at,
        preview: '',
        tool: { toolName: e.tool_name, toolUseId: e.tool_use_id, inputSummary: toolInputSummary(e.tool_name, p.input), result },
      });
    }
  }
  return cards;
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/streamModel.test.ts`
Expected: PASS (all tests).

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/streamModel.ts webui/src/components/replay/stream/__tests__/streamModel.test.ts
git commit -m "webui(redesign-v2) R2: buildStreamCards event→card mapper"
```

---

### Task 3: StreamCard component (TDD)

**Files:**
- Create: `webui/src/components/replay/stream/StreamCard.tsx`, `StreamCard.module.css`
- Test: `webui/src/components/replay/stream/__tests__/StreamCard.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/stream/__tests__/StreamCard.test.tsx
/**
 * R2 RED — StreamCard renders one chat card with actor icon, time, preview,
 * tool summary, error badge, finding marker, and episode chip. Spec §3.2.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { StreamCard } from '../StreamCard';
import type { StreamCard as Card } from '../streamModel';

function card(p: Partial<Card>): Card {
  return { id: 'c', eventId: 'c', kind: 'user', actor: 'user', timestamp: '2026-05-28T09:14:02Z', preview: 'hello', tool: null, ...p };
}

describe('StreamCard', () => {
  it('renders the preview text and a kind label for a user card', () => {
    render(<StreamCard card={card({ kind: 'user', preview: 'fix it' })} selected={false} episodePhase={null} hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText('fix it')).toBeInTheDocument();
    expect(screen.getByText(/user/i)).toBeInTheDocument();
  });

  it('shows tool name and input summary for a tool card', () => {
    render(<StreamCard card={card({ kind: 'tool', preview: '', tool: { toolName: 'Bash', toolUseId: 't', inputSummary: 'cargo test', result: { isError: false, preview: 'ok' } } })} selected={false} episodePhase={null} hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
    expect(screen.getByText('cargo test')).toBeInTheDocument();
  });

  it('shows an error badge when the tool result errored', () => {
    render(<StreamCard card={card({ kind: 'tool', tool: { toolName: 'Edit', toolUseId: 't', inputSummary: 'a.ts', result: { isError: true, preview: 'boom' } } })} selected={false} episodePhase={null} hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText(/error/i)).toBeInTheDocument();
  });

  it('shows a finding marker when hasFinding is true', () => {
    render(<StreamCard card={card({})} selected={false} episodePhase={null} hasFinding onSelect={() => {}} />);
    expect(screen.getByLabelText(/finding/i)).toBeInTheDocument();
  });

  it('shows the episode phase chip when provided', () => {
    render(<StreamCard card={card({})} selected={false} episodePhase="repair" hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText('repair')).toBeInTheDocument();
  });

  it('marks the selected card and fires onSelect with the event id on click', () => {
    const onSelect = vi.fn();
    render(<StreamCard card={card({ eventId: 'evt-1' })} selected onSelect={onSelect} episodePhase={null} hasFinding={false} />);
    const el = screen.getByTestId('stream-card');
    expect(el.getAttribute('data-selected')).toBe('true');
    fireEvent.click(el);
    expect(onSelect).toHaveBeenCalledWith('evt-1');
  });
});
```

- [ ] **Step 2: Run, verify fail**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/StreamCard.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// webui/src/components/replay/stream/StreamCard.tsx
import { User, Bot, BrainCog, Wrench, Link2 } from 'lucide-react';
import type { StreamCard as Card, StreamCardKind } from './streamModel';
import styles from './StreamCard.module.css';

const KIND_META: Record<StreamCardKind, { label: string; Icon: typeof User }> = {
  user: { label: 'User', Icon: User },
  assistant: { label: 'Assistant', Icon: Bot },
  thinking: { label: 'Thinking', Icon: BrainCog },
  tool: { label: 'Tool', Icon: Wrench },
};

function timeLabel(iso: string): string {
  // HH:MM:SS in the viewer's locale; fall back to the raw string.
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().slice(11, 19);
}

interface StreamCardProps {
  card: Card;
  selected: boolean;
  episodePhase: string | null;
  hasFinding: boolean;
  onSelect: (eventId: string) => void;
}

export function StreamCard({ card, selected, episodePhase, hasFinding, onSelect }: StreamCardProps) {
  const meta = KIND_META[card.kind];
  const Icon = meta.Icon;
  return (
    <div
      data-testid="stream-card"
      data-kind={card.kind}
      data-selected={selected ? 'true' : 'false'}
      className={`${styles.card} ${styles[card.kind]} ${selected ? styles.selected : ''}`}
      onClick={() => onSelect(card.eventId)}
    >
      <div className={styles.head}>
        <span className={styles.kind}>
          <Icon size={14} aria-hidden className={styles.icon} />
          {meta.label}
        </span>
        <span className={styles.time}>{timeLabel(card.timestamp)}</span>
        {episodePhase && <span className={styles.phase} data-phase={episodePhase}>{episodePhase}</span>}
        {hasFinding && <Link2 size={13} aria-label="linked finding" className={styles.finding} />}
      </div>

      {card.kind === 'tool' && card.tool ? (
        <div className={styles.toolBody}>
          <span className={styles.toolName}>{card.tool.toolName}</span>
          {card.tool.inputSummary && <code className={styles.toolArg}>{card.tool.inputSummary}</code>}
          {card.tool.result && (
            <span className={card.tool.result.isError ? styles.badgeError : styles.badgeOk}>
              {card.tool.result.isError ? 'error' : 'ok'}
            </span>
          )}
        </div>
      ) : (
        <p className={styles.preview}>{card.preview}</p>
      )}
    </div>
  );
}
```

```css
/* webui/src/components/replay/stream/StreamCard.module.css */
.card { border-left: 3px solid var(--witmcc-border-strong, #2a3040); background: var(--witmcc-surface, #161a22); padding: 6px 10px; margin: 6px 0; border-radius: 4px; cursor: pointer; }
.card[data-selected='true'], .selected { outline: 1px solid var(--witmcc-accent, #4f8cff); }
.user { border-left-color: #4f8cff; }
.assistant { border-left-color: #4ec98a; }
.thinking { border-left-color: #b483f0; }
.tool { border-left-color: #e2b148; }
.head { display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--witmcc-fg-muted, #aab0bd); }
.kind { display: inline-flex; align-items: center; gap: 4px; font-weight: 600; }
.icon { flex: none; }
.time { color: var(--witmcc-fg-subtle, #6a7180); }
.phase { margin-left: auto; padding: 1px 6px; border-radius: 3px; background: var(--witmcc-accent-soft, #1f3a78); color: var(--witmcc-fg, #e6e8ee); text-transform: lowercase; }
.finding { color: var(--witmcc-accent, #4f8cff); }
.preview { margin: 4px 0 0; color: var(--witmcc-fg, #e6e8ee); white-space: pre-wrap; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; }
.toolBody { display: flex; align-items: center; gap: 8px; margin-top: 4px; flex-wrap: wrap; }
.toolName { font-weight: 600; color: var(--witmcc-fg, #e6e8ee); }
.toolArg { font-family: var(--witmcc-mono, ui-monospace, monospace); font-size: 11px; color: var(--witmcc-fg-muted, #aab0bd); overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
.badgeOk { font-size: 10px; padding: 1px 5px; border-radius: 2px; background: #1f5135; color: #9fe7bf; }
.badgeError { font-size: 10px; padding: 1px 5px; border-radius: 2px; background: #5a1f23; color: #ffb3b8; }
```

- [ ] **Step 4: Run, verify pass**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/StreamCard.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/StreamCard.tsx webui/src/components/replay/stream/StreamCard.module.css webui/src/components/replay/stream/__tests__/StreamCard.test.tsx
git commit -m "webui(redesign-v2) R2: StreamCard component with lucide icons"
```

---

### Task 4: ConversationStream container + wire-in (TDD)

**Files:**
- Create: `webui/src/components/replay/stream/ConversationStream.tsx`, `ConversationStream.module.css`
- Test: `webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx`
- Modify: `webui/src/routes/SessionDetailPage.tsx`

The component takes already-built cards plus lookup maps and renders them in a virtualized, scrollable list. To keep jsdom tests deterministic, the host builds the `eventId→nodeId`, `eventId→phase`, and `nodeId→hasFinding` maps and the cards; the component is presentational over `cards`.

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx
/**
 * R2 RED — ConversationStream renders cards oldest→newest (newest at the
 * DOM bottom), forwards clicks, and reflects selection. Spec §3.
 */
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ConversationStream } from '../ConversationStream';
import type { StreamCard } from '../streamModel';

function c(id: string, preview: string): StreamCard {
  return { id, eventId: id, kind: 'user', actor: 'user', timestamp: '2026-05-28T09:14:02Z', preview, tool: null };
}

describe('ConversationStream', () => {
  it('renders one card per item in source order (newest last in the DOM)', () => {
    render(
      <ConversationStream
        cards={[c('a', 'first'), c('b', 'second')]}
        selectedEventId={null}
        phaseByEventId={{}}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const cards = screen.getAllByTestId('stream-card');
    expect(cards).toHaveLength(2);
    expect(within(cards[0]).getByText('first')).toBeInTheDocument();
    expect(within(cards[1]).getByText('second')).toBeInTheDocument();
  });

  it('marks the selected card', () => {
    render(
      <ConversationStream
        cards={[c('a', 'first'), c('b', 'second')]}
        selectedEventId="b"
        phaseByEventId={{}}
        findingEventIds={new Set()}
        onSelect={() => {}}
      />,
    );
    const cards = screen.getAllByTestId('stream-card');
    expect(cards[1].getAttribute('data-selected')).toBe('true');
    expect(cards[0].getAttribute('data-selected')).toBe('false');
  });

  it('passes the episode phase and finding marker through to the card', () => {
    render(
      <ConversationStream
        cards={[c('a', 'first')]}
        selectedEventId={null}
        phaseByEventId={{ a: 'repair' }}
        findingEventIds={new Set(['a'])}
        onSelect={() => {}}
      />,
    );
    expect(screen.getByText('repair')).toBeInTheDocument();
    expect(screen.getByLabelText(/finding/i)).toBeInTheDocument();
  });

  it('renders an empty hint when there are no cards', () => {
    render(
      <ConversationStream cards={[]} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={() => {}} />,
    );
    expect(screen.getByText(/no conversation/i)).toBeInTheDocument();
  });

  it('forwards clicks with the event id', () => {
    const onSelect = vi.fn();
    render(
      <ConversationStream cards={[c('a', 'first')]} selectedEventId={null} phaseByEventId={{}} findingEventIds={new Set()} onSelect={onSelect} />,
    );
    screen.getByTestId('stream-card').click();
    expect(onSelect).toHaveBeenCalledWith('a');
  });
});
```

- [ ] **Step 2: Run, verify fail**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/ConversationStream.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Note: `@tanstack/react-virtual`'s measurement relies on layout that jsdom does not implement; to keep tests meaningful and avoid mocking, render all cards but cap mounted DOM via the virtualizer **only when a scroll element with a measured height exists**. The simplest robust approach that satisfies both tests and the memory goal: use the virtualizer with `useWindowVirtualizer`-free `useVirtualizer` over a scroll container, and when `getVirtualItems()` is empty (jsdom, zero height) fall back to rendering all cards. Implement:

```tsx
// webui/src/components/replay/stream/ConversationStream.tsx
import { useEffect, useLayoutEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { StreamCard } from './StreamCard';
import type { StreamCard as Card } from './streamModel';
import styles from './ConversationStream.module.css';

interface ConversationStreamProps {
  cards: Card[];
  selectedEventId: string | null;
  phaseByEventId: Record<string, string>;
  findingEventIds: Set<string>;
  onSelect: (eventId: string) => void;
}

export function ConversationStream({ cards, selectedEventId, phaseByEventId, findingEventIds, onSelect }: ConversationStreamProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const atBottomRef = useRef(true);

  const virtualizer = useVirtualizer({
    count: cards.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 64,
    overscan: 8,
  });

  const virtualItems = virtualizer.getVirtualItems();
  // jsdom / zero-height container: the virtualizer yields no items. Render all
  // cards so behavior is observable in tests and on first paint before measure.
  const useVirtual = virtualItems.length > 0;

  // Track whether the user is pinned to the bottom so live appends autoscroll
  // only when they haven't scrolled up to read history.
  const onScroll = () => {
    const el = parentRef.current;
    if (!el) return;
    atBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  useLayoutEffect(() => {
    const el = parentRef.current;
    if (el && atBottomRef.current) el.scrollTop = el.scrollHeight;
  }, [cards.length]);

  useEffect(() => {
    // keep virtualizer range fresh when the set grows
    if (atBottomRef.current && cards.length > 0) virtualizer.scrollToIndex(cards.length - 1);
  }, [cards.length, virtualizer]);

  if (cards.length === 0) {
    return <p className={styles.empty}>No conversation events yet.</p>;
  }

  const renderCard = (card: Card) => (
    <StreamCard
      key={card.id}
      card={card}
      selected={card.eventId === selectedEventId}
      episodePhase={phaseByEventId[card.eventId] ?? null}
      hasFinding={findingEventIds.has(card.eventId)}
      onSelect={onSelect}
    />
  );

  return (
    <div ref={parentRef} className={styles.scroll} onScroll={onScroll} data-testid="conversation-stream">
      {useVirtual ? (
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualItems.map((vi) => {
            const card = cards[vi.index];
            return (
              <div
                key={card.id}
                ref={virtualizer.measureElement}
                data-index={vi.index}
                style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${vi.start}px)` }}
              >
                {renderCard(card)}
              </div>
            );
          })}
        </div>
      ) : (
        cards.map(renderCard)
      )}
    </div>
  );
}
```

```css
/* webui/src/components/replay/stream/ConversationStream.module.css */
.scroll { height: 100%; min-height: 320px; overflow-y: auto; padding-right: 4px; }
.empty { color: var(--witmcc-fg-subtle, #6a7180); padding: 12px; }
```

- [ ] **Step 4: Run, verify pass**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/ConversationStream.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 5: Wire into SessionDetailPage**

In `webui/src/routes/SessionDetailPage.tsx`:

Add imports:
```tsx
import { ConversationStream } from '../components/replay/stream/ConversationStream';
import { buildStreamCards } from '../components/replay/stream/streamModel';
```

Inside `SessionDetailInner`, after the existing derived values, add:
```tsx
  const streamCards = useMemo(() => buildStreamCards(window_.events), [window_.events]);

  // event_id -> node_id (graph nodes carry source_event_ids)
  const nodeIdByEventId = useMemo(() => {
    const m = new Map<string, string>();
    for (const n of effectiveGraph.nodes) for (const eid of n.source_event_ids) m.set(eid, n.node_id);
    return m;
  }, [effectiveGraph]);

  // event ids that have a finding (finding.evidence_refs -> node -> source events)
  const findingEventIds = useMemo(() => {
    const nodeIdsWithFinding = new Set<string>();
    for (const f of findingsData) {
      for (const ref of f.evidence_refs) {
        const id = typeof ref === 'string' ? ref : ref.node_id ?? ref.event_id;
        if (typeof id === 'string') nodeIdsWithFinding.add(id);
      }
    }
    const eids = new Set<string>();
    for (const n of effectiveGraph.nodes) {
      if (nodeIdsWithFinding.has(n.node_id)) for (const eid of n.source_event_ids) eids.add(eid);
    }
    // some refs are event ids directly
    for (const f of findingsData) for (const ref of f.evidence_refs) {
      const id = typeof ref === 'string' ? ref : ref.event_id;
      if (typeof id === 'string') eids.add(id);
    }
    return eids;
  }, [findingsData, effectiveGraph]);

  // event_id -> episode phase (by observed_at within [started_at, ended_at])
  const phaseByEventId = useMemo(() => {
    const eps = episodes.data ?? [];
    const out: Record<string, string> = {};
    for (const card of streamCards) {
      const t = card.timestamp;
      const ep = eps.find((e) => e.started_at <= t && t <= e.ended_at);
      if (ep) out[card.eventId] = ep.phase;
    }
    return out;
  }, [streamCards, episodes.data]);

  const selectedStreamEventId = useMemo(() => {
    if (!sel.selectedNodeId) return null;
    const n = effectiveGraph.nodes.find((x) => x.node_id === sel.selectedNodeId);
    return n?.source_event_ids[0] ?? null;
  }, [sel.selectedNodeId, effectiveGraph]);

  const selectStreamCard = (eventId: string) => {
    const nid = nodeIdByEventId.get(eventId);
    sel.setSelectedNodeId(nid ?? null);
  };
```

Replace the `stream` slot's placeholder `<p className={styles.placeholder}>Conversation stream (R2)</p>` with:
```tsx
            <ConversationStream
              cards={streamCards}
              selectedEventId={selectedStreamEventId}
              phaseByEventId={phaseByEventId}
              findingEventIds={findingEventIds}
              onSelect={selectStreamCard}
            />
```

Keep the scroll-sentinel `<div>` above it (loadOlder still works).

- [ ] **Step 6: Run full suite + build + smoke prep**

Run: `cd webui && npx vitest run && npx tsc --noEmit && npm run build`
Expected: all green. Fix any SessionDetailPage test that asserted the old placeholder text (update it to assert the stream renders).

- [ ] **Step 7: Commit**

```bash
git add webui/src/components/replay/stream/ConversationStream.tsx webui/src/components/replay/stream/ConversationStream.module.css webui/src/components/replay/stream/__tests__/ConversationStream.test.tsx webui/src/routes/SessionDetailPage.tsx webui/src/routes/__tests__/SessionDetailPage.test.tsx
git commit -m "webui(redesign-v2) R2: ConversationStream + wire into stream slot"
```

---

### Task 5: Browser smoke

- [ ] **Step 1: Rebuild + serve**

Run: `cd webui && npm run build && cd .. && cargo build && ./target/debug/witmcc serve --port 7878 --no-watch-transcripts &`

- [ ] **Step 2: Verify in browser** (claude-in-chrome) on `http://127.0.0.1:7878/sessions/<id>`:
- Left slot shows chat cards: User / Assistant / Thinking / Tool, in time order, newest at the bottom, scrollable.
- Tool cards show tool name + arg summary + ok/error badge.
- Clicking a card selects it (and updates the right panel / timeline node).
- A session with findings shows the finding marker; episode chips appear.

- [ ] **Step 3:** Fix any issues, re-run. When clean, R2 done.

---

## Self-Review

- **Spec coverage:** §3 card types (4), §3.2 inline fields 1–5,7,8 inline; field 6 (token badge) explicitly deferred to R3 with a real-data reason (usage only in raw record). Real-data anchor documented. Virtualization present (Task 4).
- **Placeholder scan:** No "TBD"/stub; every step has full code. The "(thinking redacted)" preview is a real user-facing string, tested.
- **Type consistency:** `StreamCard` type fields (`id`, `eventId`, `kind`, `actor`, `timestamp`, `preview`, `tool`) used identically across mapper, card, container, and host. `buildStreamCards`, `ConversationStream` prop names match between tests and impl.
