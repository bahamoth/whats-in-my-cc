# Redesign v2 — R3: Detail Panel (Insight / Detail / Raw tabs) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans. Checkbox steps.

**Goal:** Replace the raw-JSON `SourcePanel` in the `detail` slot with a tabbed `DetailPanel` — **Insight** (findings for the selected node, absorbing the old WhyPanel), **Detail** (human-readable fields incl. the deferred token badge), **Raw** (JSON whose expansion survives refreshes). Demote raw JSON from prime real estate to one on-demand tab.

**Architecture:** `DetailPanel` owns tab state (persisted per session, default Insight→fallback Detail). It receives the selected graph node, its raw record (lazily fetched, reused across Detail+Raw tabs), and the findings referencing it. The Raw tab renders a **custom controlled `JsonTree`** whose expansion set lives in React state keyed by node id, so SSE appends / refetches never collapse it (fixes feedback #2 — react-json-view-lite offers no controlled expansion, only initial `shouldExpandNode`, and its tree unmounts during the SourcePanel `loading` flip). The token badge (deferred from R2) renders in the Detail tab from `raw.message.usage`, which is already fetched there.

**Tech Stack:** React 18, TypeScript, lucide-react, CSS Modules, Vitest + Testing Library. Removes `react-json-view-lite` usage (replaced by `JsonTree`); the dep can stay installed (other code may use it) — verify and remove import only.

**Spec:** `docs/superpowers/specs/2026-05-29-witmcc-ux-redesign-v2-design.md` §4. Resolves feedback #1 (raw demoted), #2 (tree persistence), and the deferred token badge.

**Real-data anchor (live API, session e301d123):**
- Raw assistant record: `GET /v1/events/:id/raw` → `record_type:'assistant_message'`, `record.message.usage:{ input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens, ... }`.
- `tool_result.tool_result.content` is the output; `is_error` only when erroring.
- Finding: `{ finding_id, category, severity, confidence, summary, evidence_refs[], ... }` (from `FindingDto`).
- Existing endpoints: `getEventRaw(eventId)` (client.ts), `useFindingEvidenceQuery`, findings via `useFindingsQuery`.

---

## File Structure

- **Create** `webui/src/components/replay/detail/JsonTree.tsx` + `.module.css` — custom controlled collapsible JSON tree. One responsibility: render JSON with externally-controlled expansion.
- **Create** `webui/src/components/replay/detail/__tests__/JsonTree.test.tsx`.
- **Create** `webui/src/components/replay/detail/RawTab.tsx` — wraps JsonTree with per-node persistent expansion state.
- **Create** `webui/src/components/replay/detail/__tests__/RawTab.test.tsx`.
- **Create** `webui/src/components/replay/detail/DetailTab.tsx` + `.module.css` — structured fields + token badge.
- **Create** `webui/src/components/replay/detail/__tests__/DetailTab.test.tsx`.
- **Create** `webui/src/components/replay/detail/InsightTab.tsx` + `.module.css` — findings list (absorbs WhyPanel content).
- **Create** `webui/src/components/replay/detail/__tests__/InsightTab.test.tsx`.
- **Create** `webui/src/components/replay/detail/DetailPanel.tsx` + `.module.css` — tab shell + persistence.
- **Create** `webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx`.
- **Modify** `webui/src/routes/SessionDetailPage.tsx` — mount `DetailPanel` in the detail slot; remove `SourcePanel` placement and the `WhyPanel` drawer.

---

### Task 1: Controlled JSON tree (TDD) — the #2 fix core

A small recursive renderer. Expansion is **fully controlled**: the parent passes `expanded: Set<string>` (set of dot/bracket paths that are open) and `onToggle(path)`. Because state lives in the parent (and the parent keys it by node id), re-renders and refetches never reset it.

**Files:** Create `webui/src/components/replay/detail/JsonTree.tsx`, `JsonTree.module.css`; Test `__tests__/JsonTree.test.tsx`.

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/detail/__tests__/JsonTree.test.tsx
/**
 * R3 RED — JsonTree is a controlled collapsible JSON renderer. Expansion is
 * owned by the parent (Set<string> of open paths) so it survives re-render.
 * Plan R3 Task 1 / spec §4 (#2 persistence).
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { JsonTree } from '../JsonTree';

describe('JsonTree', () => {
  it('renders primitive leaves with their key and value', () => {
    render(<JsonTree data={{ a: 1, b: 'x' }} expanded={new Set(['$'])} onToggle={() => {}} />);
    expect(screen.getByText('a')).toBeInTheDocument();
    expect(screen.getByText('1')).toBeInTheDocument();
    expect(screen.getByText('b')).toBeInTheDocument();
    expect(screen.getByText('"x"')).toBeInTheDocument();
  });

  it('hides children of a collapsed object', () => {
    // root '$' open, but nested '$.obj' closed
    render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$'])} onToggle={() => {}} />);
    expect(screen.getByText('obj')).toBeInTheDocument();
    expect(screen.queryByText('deep')).toBeNull();
  });

  it('shows children when the path is in the expanded set', () => {
    render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$', '$.obj'])} onToggle={() => {}} />);
    expect(screen.getByText('deep')).toBeInTheDocument();
  });

  it('fires onToggle with the node path when a collapsible key is clicked', () => {
    const onToggle = vi.fn();
    render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$'])} onToggle={onToggle} />);
    fireEvent.click(screen.getByText('obj'));
    expect(onToggle).toHaveBeenCalledWith('$.obj');
  });

  it('preserves displayed expansion across a re-render with a new data reference (same shape)', () => {
    const { rerender } = render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$', '$.obj'])} onToggle={() => {}} />);
    expect(screen.getByText('deep')).toBeInTheDocument();
    // simulate a refetch handing a brand-new object with identical shape
    rerender(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$', '$.obj'])} onToggle={() => {}} />);
    expect(screen.getByText('deep')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail** — `cd webui && npx vitest run src/components/replay/detail/__tests__/JsonTree.test.tsx` → FAIL (module not found).

- [ ] **Step 3: Implement**

```tsx
// webui/src/components/replay/detail/JsonTree.tsx
import { ChevronRight, ChevronDown } from 'lucide-react';
import styles from './JsonTree.module.css';

export interface JsonTreeProps {
  data: unknown;
  /** Set of open paths. Root path is "$". */
  expanded: Set<string>;
  onToggle: (path: string) => void;
}

function isContainer(v: unknown): v is object {
  return v !== null && typeof v === 'object';
}

function formatPrimitive(v: unknown): string {
  if (typeof v === 'string') return `"${v}"`;
  if (v === null) return 'null';
  return String(v);
}

function Node({ k, value, path, expanded, onToggle }: { k: string | null; value: unknown; path: string; expanded: Set<string>; onToggle: (p: string) => void }) {
  const container = isContainer(value);
  const open = expanded.has(path);

  if (!container) {
    return (
      <div className={styles.row}>
        {k !== null && <span className={styles.key}>{k}</span>}
        {k !== null && <span className={styles.colon}>:</span>}
        <span className={styles.value}>{formatPrimitive(value)}</span>
      </div>
    );
  }

  const entries = Array.isArray(value)
    ? value.map((v, i) => [String(i), v] as const)
    : Object.entries(value as Record<string, unknown>);
  const label = k ?? '$';
  const Chevron = open ? ChevronDown : ChevronRight;

  return (
    <div className={styles.node}>
      <div className={styles.row}>
        <button type="button" className={styles.toggle} onClick={() => onToggle(path)} aria-expanded={open}>
          <Chevron size={12} aria-hidden />
          <span className={styles.key}>{label}</span>
          <span className={styles.preview}>{Array.isArray(value) ? `[${entries.length}]` : `{${entries.length}}`}</span>
        </button>
      </div>
      {open && (
        <div className={styles.children}>
          {entries.map(([childKey, childVal]) => (
            <Node key={childKey} k={childKey} value={childVal} path={`${path}.${childKey}`} expanded={expanded} onToggle={onToggle} />
          ))}
        </div>
      )}
    </div>
  );
}

export function JsonTree({ data, expanded, onToggle }: JsonTreeProps) {
  return (
    <div className={styles.tree}>
      <Node k={null} value={isContainer(data) ? data : { value: data }} path="$" expanded={expanded} onToggle={onToggle} />
    </div>
  );
}
```

Wait — the root Node must use key label "$" and be a container; if `data` is a primitive we wrapped it. But the leaf tests pass `{a:1,b:'x'}` with expanded `['$']`, so root is open and shows a,b. Good. The "obj" click test expects path `$.obj`. Good.

```css
/* webui/src/components/replay/detail/JsonTree.module.css */
.tree { font-family: var(--witmcc-mono, ui-monospace, monospace); font-size: 12px; line-height: 1.6; }
.row { display: flex; align-items: center; gap: 4px; }
.toggle { display: inline-flex; align-items: center; gap: 4px; background: none; border: none; color: inherit; cursor: pointer; padding: 0; font: inherit; }
.children { padding-left: 14px; border-left: 1px solid var(--witmcc-border, #1d212c); margin-left: 5px; }
.key { color: var(--witmcc-accent, #4f8cff); }
.colon { color: var(--witmcc-fg-subtle, #6a7180); }
.value { color: var(--witmcc-fg, #e6e8ee); white-space: pre-wrap; word-break: break-word; }
.preview { color: var(--witmcc-fg-subtle, #6a7180); }
```

- [ ] **Step 4: Run, verify pass** — all JsonTree tests green.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/detail/JsonTree.tsx webui/src/components/replay/detail/JsonTree.module.css webui/src/components/replay/detail/__tests__/JsonTree.test.tsx
git commit -m "webui(redesign-v2) R3: controlled JsonTree"
```

---

### Task 2: RawTab with per-node persistent expansion (TDD)

`RawTab` owns the expansion `Set<string>` and keeps a **separate set per node id** in a ref-backed map, so navigating away and back, and (critically) data refreshes, preserve the open paths. Default-open the root.

**Files:** Create `webui/src/components/replay/detail/RawTab.tsx`; Test `__tests__/RawTab.test.tsx`.

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/detail/__tests__/RawTab.test.tsx
/**
 * R3 RED — RawTab persists JsonTree expansion per node across re-renders /
 * data refreshes (the #2 regression lock). Plan R3 Task 2 / spec §4.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RawTab } from '../RawTab';

describe('RawTab', () => {
  it('renders the raw record as a tree (root open by default)', () => {
    render(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    expect(screen.getByText('outer')).toBeInTheDocument();
  });

  it('keeps a node expanded after a re-render with a new record reference', () => {
    const { rerender } = render(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    fireEvent.click(screen.getByText('outer')); // expand $.outer
    expect(screen.getByText('inner')).toBeInTheDocument();
    // refetch hands a fresh object of the same shape
    rerender(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    expect(screen.getByText('inner')).toBeInTheDocument();
  });

  it('shows an empty hint when there is no record', () => {
    render(<RawTab nodeId={null} record={null} />);
    expect(screen.getByText(/no raw record|select/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement**

```tsx
// webui/src/components/replay/detail/RawTab.tsx
import { useRef, useState } from 'react';
import { JsonTree } from './JsonTree';
import styles from './JsonTree.module.css';

interface RawTabProps {
  nodeId: string | null;
  record: unknown;
}

export function RawTab({ nodeId, record }: RawTabProps) {
  // expansion sets keyed by node id; survives re-render and data refresh
  const store = useRef<Map<string, Set<string>>>(new Map());
  const [, force] = useState(0);

  if (record == null) {
    return <p className={styles.empty}>No raw record — select a node.</p>;
  }

  const key = nodeId ?? '$anon';
  let set = store.current.get(key);
  if (!set) {
    set = new Set<string>(['$']); // root open by default
    store.current.set(key, set);
  }

  const onToggle = (path: string) => {
    const s = store.current.get(key)!;
    if (s.has(path)) s.delete(path);
    else s.add(path);
    force((n) => n + 1);
  };

  return <JsonTree data={record} expanded={set} onToggle={onToggle} />;
}
```

Add to `JsonTree.module.css`: `.empty { color: var(--witmcc-fg-subtle, #6a7180); padding: 12px; }`

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/detail/RawTab.tsx webui/src/components/replay/detail/JsonTree.module.css webui/src/components/replay/detail/__tests__/RawTab.test.tsx
git commit -m "webui(redesign-v2) R3: RawTab with per-node persistent expansion"
```

---

### Task 3: DetailTab — structured fields + token badge (TDD)

Presentational: takes the selected graph node + its raw record + episode phase, renders human-readable rows. Token usage from `record.message.usage` when present.

**Files:** Create `webui/src/components/replay/detail/DetailTab.tsx`, `.module.css`; Test `__tests__/DetailTab.test.tsx`.

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/detail/__tests__/DetailTab.test.tsx
/**
 * R3 RED — DetailTab renders human-readable fields and a token badge from
 * the raw record's usage. Plan R3 Task 3 / spec §4.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailTab } from '../DetailTab';

const node = {
  node_kind: 'assistant_message',
  started_at: '2026-05-28T09:14:08Z',
  ended_at: '2026-05-28T09:14:10Z',
} as any;

describe('DetailTab', () => {
  it('shows the node kind and timestamp', () => {
    render(<DetailTab node={node} record={null} episodePhase={null} />);
    expect(screen.getByText('assistant_message')).toBeInTheDocument();
  });

  it('shows a token badge when usage is present in the raw record', () => {
    render(<DetailTab node={node} record={{ message: { usage: { output_tokens: 451, input_tokens: 6, cache_read_input_tokens: 59055 } } }} episodePhase={null} />);
    expect(screen.getByText(/451/)).toBeInTheDocument();
    expect(screen.getByText(/out/i)).toBeInTheDocument();
  });

  it('shows tool name and error state for a tool node record', () => {
    render(<DetailTab node={{ node_kind: 'tool_call', started_at: node.started_at, ended_at: null } as any} record={{ tool_result: { is_error: true, content: 'boom' } }} episodePhase="repair" />);
    expect(screen.getByText('tool_call')).toBeInTheDocument();
    expect(screen.getByText('repair')).toBeInTheDocument();
    expect(screen.getByText(/error/i)).toBeInTheDocument();
  });

  it('shows an empty hint with no node', () => {
    render(<DetailTab node={null} record={null} episodePhase={null} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement**

```tsx
// webui/src/components/replay/detail/DetailTab.tsx
import type { GraphNodeDto } from '../../../api/types';
import styles from './DetailTab.module.css';

interface DetailTabProps {
  node: GraphNodeDto | null;
  record: unknown;
  episodePhase: string | null;
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function fmtTime(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().replace('T', ' ').slice(0, 19);
}

export function DetailTab({ node, record, episodePhase }: DetailTabProps) {
  if (!node) return <p className={styles.empty}>Select a node to see its details.</p>;

  const rec = asObj(record);
  const usage = asObj(asObj(rec.message).usage);
  const toolResult = asObj(rec.tool_result);
  const hasUsage = Object.keys(usage).length > 0;
  const isError = toolResult.is_error === true;

  const rows: Array<[string, string]> = [
    ['kind', node.node_kind],
    ['started', fmtTime(node.started_at)],
    ['ended', fmtTime(node.ended_at)],
  ];
  if (episodePhase) rows.push(['episode', episodePhase]);

  return (
    <div className={styles.detail}>
      <table className={styles.table}>
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <td className={styles.k}>{k}</td>
              <td className={styles.v}>{k === 'episode' ? <span className={styles.phase}>{v}</span> : v}</td>
            </tr>
          ))}
          {isError && (
            <tr><td className={styles.k}>result</td><td className={styles.v}><span className={styles.error}>error</span></td></tr>
          )}
        </tbody>
      </table>

      {hasUsage && (
        <div className={styles.usage} aria-label="token usage">
          <span className={styles.badge}>out {String(usage.output_tokens ?? '—')}</span>
          <span className={styles.badge}>in {String(usage.input_tokens ?? '—')}</span>
          {usage.cache_read_input_tokens != null && <span className={styles.badge}>cache {String(usage.cache_read_input_tokens)}</span>}
        </div>
      )}
    </div>
  );
}
```

```css
/* webui/src/components/replay/detail/DetailTab.module.css */
.detail { padding: 4px; }
.empty { color: var(--witmcc-fg-subtle, #6a7180); padding: 12px; }
.table { border-collapse: collapse; width: 100%; }
.k { color: var(--witmcc-fg-subtle, #6a7180); padding: 3px 12px 3px 0; vertical-align: top; width: 90px; }
.v { color: var(--witmcc-fg, #e6e8ee); padding: 3px 0; font-family: var(--witmcc-mono, ui-monospace, monospace); }
.phase { padding: 1px 6px; border-radius: 3px; background: var(--witmcc-accent-soft, #1f3a78); }
.error { padding: 1px 6px; border-radius: 3px; border: 1px solid var(--witmcc-danger, #ef4747); color: var(--witmcc-danger, #ef4747); }
.usage { display: flex; gap: 6px; margin-top: 10px; flex-wrap: wrap; }
.badge { font-size: 11px; padding: 2px 7px; border-radius: 3px; background: var(--witmcc-surface-3, #1c212b); color: var(--witmcc-fg-muted, #aab0bd); }
```

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/detail/DetailTab.tsx webui/src/components/replay/detail/DetailTab.module.css webui/src/components/replay/detail/__tests__/DetailTab.test.tsx
git commit -m "webui(redesign-v2) R3: DetailTab structured fields + token badge"
```

---

### Task 4: InsightTab — findings for the selected node (TDD)

Absorbs the WhyPanel's job: list findings whose evidence references the selected node, with category/severity/confidence/summary. (R5 will add the focused subgraph below this list.)

**Files:** Create `webui/src/components/replay/detail/InsightTab.tsx`, `.module.css`; Test `__tests__/InsightTab.test.tsx`.

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * R3 RED — InsightTab lists findings linked to the selected node. Absorbs the
 * WhyPanel. Plan R3 Task 4 / spec §4.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InsightTab } from '../InsightTab';
import type { FindingDto } from '../../../api/types';

function finding(p: Partial<FindingDto>): FindingDto {
  return {
    finding_id: 'f1', schema_version: '1', session_id: 's', category: 'risky_action',
    severity: 'high', confidence: 0.8, summary: 'risky rm -rf', evidence_refs: [],
    evidence_projection: {}, provenance: {}, status: 'open', created_at: '', ...p,
  };
}

describe('InsightTab', () => {
  it('renders each finding with summary, category, and severity', () => {
    render(<InsightTab findings={[finding({})]} />);
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
    expect(screen.getByText(/risky_action/)).toBeInTheDocument();
    expect(screen.getByText(/high/i)).toBeInTheDocument();
  });

  it('renders confidence as a percentage', () => {
    render(<InsightTab findings={[finding({ confidence: 0.8 })]} />);
    expect(screen.getByText('80%')).toBeInTheDocument();
  });

  it('shows an empty hint when the node has no findings', () => {
    render(<InsightTab findings={[]} />);
    expect(screen.getByText(/no insights|no findings/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement**

```tsx
// webui/src/components/replay/detail/InsightTab.tsx
import type { FindingDto } from '../../../api/types';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  findings: FindingDto[];
}

const SEV_CLASS: Record<string, string> = { high: 'sevHigh', medium: 'sevMed', low: 'sevLow' };

export function InsightTab({ findings }: InsightTabProps) {
  if (findings.length === 0) {
    return <p className={styles.empty}>No insights for this node.</p>;
  }
  return (
    <ul className={styles.list}>
      {findings.map((f) => (
        <li key={f.finding_id} className={styles.item}>
          <div className={styles.head}>
            <span className={`${styles.sev} ${styles[SEV_CLASS[f.severity] ?? 'sevLow']}`}>{f.severity}</span>
            <span className={styles.category}>{f.category}</span>
            <span className={styles.confidence}>{Math.round(f.confidence * 100)}%</span>
          </div>
          <p className={styles.summary}>{f.summary}</p>
        </li>
      ))}
    </ul>
  );
}
```

```css
/* webui/src/components/replay/detail/InsightTab.module.css */
.empty { color: var(--witmcc-fg-subtle, #6a7180); padding: 12px; }
.list { list-style: none; margin: 0; padding: 0; }
.item { border: 1px solid var(--witmcc-border, #1d212c); border-radius: 4px; padding: 8px 10px; margin-bottom: 8px; }
.head { display: flex; align-items: center; gap: 8px; font-size: 11px; }
.sev { padding: 1px 6px; border-radius: 3px; text-transform: uppercase; font-weight: 600; }
.sevHigh { background: var(--witmcc-danger, #ef4747); color: #fff; }
.sevMed { background: var(--witmcc-warning, #e2b148); color: #14181f; }
.sevLow { background: var(--witmcc-surface-3, #1c212b); color: var(--witmcc-fg-muted, #aab0bd); }
.category { color: var(--witmcc-fg-muted, #aab0bd); font-family: var(--witmcc-mono, ui-monospace, monospace); }
.confidence { margin-left: auto; color: var(--witmcc-fg-subtle, #6a7180); }
.summary { margin: 6px 0 0; color: var(--witmcc-fg, #e6e8ee); }
```

> If `--witmcc-warning` is absent in tokens.css, use the nearest existing token (e.g. an amber/lane-action token) — verify against tokens.css during implementation.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/detail/InsightTab.tsx webui/src/components/replay/detail/InsightTab.module.css webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
git commit -m "webui(redesign-v2) R3: InsightTab findings list (absorbs WhyPanel)"
```

---

### Task 5: DetailPanel shell — tabs + persistence (TDD)

Owns tab selection. Default **Insight**; if the selected node has no findings, default to **Detail**. Tab choice persists across data refresh (held in state that only resets on explicit user click, not on prop/data churn).

**Files:** Create `webui/src/components/replay/detail/DetailPanel.tsx`, `.module.css`; Test `__tests__/DetailPanel.test.tsx`.

- [ ] **Step 1: Write the failing test**

```tsx
// webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx
/**
 * R3 RED — DetailPanel hosts Insight/Detail/Raw tabs, defaults to Insight
 * (Detail when no findings), and keeps the chosen tab across re-render.
 * Plan R3 Task 5 / spec §4.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DetailPanel } from '../DetailPanel';
import type { FindingDto, GraphNodeDto } from '../../../api/types';

const node = { node_kind: 'tool_call', started_at: '2026-05-28T09:14:08Z', ended_at: null } as GraphNodeDto;
function finding(): FindingDto {
  return { finding_id: 'f1', schema_version: '1', session_id: 's', category: 'c', severity: 'high', confidence: 0.5, summary: 's', evidence_refs: [], evidence_projection: {}, provenance: {}, status: 'open', created_at: '' };
}

describe('DetailPanel', () => {
  it('renders three tabs', () => {
    render(<DetailPanel node={node} record={null} findings={[]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /detail/i })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /raw/i })).toBeInTheDocument();
  });

  it('defaults to the Insight tab when there are findings', () => {
    render(<DetailPanel node={node} record={null} findings={[finding()]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /insight/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('defaults to Detail when the node has no findings', () => {
    render(<DetailPanel node={node} record={null} findings={[]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /detail/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('switches tab on click and keeps it across a re-render', () => {
    const { rerender } = render(<DetailPanel node={node} record={{ a: 1 }} findings={[finding()]} episodePhase={null} />);
    fireEvent.click(screen.getByRole('tab', { name: /raw/i }));
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
    rerender(<DetailPanel node={node} record={{ a: 1 }} findings={[finding()]} episodePhase={null} />);
    expect(screen.getByRole('tab', { name: /raw/i })).toHaveAttribute('aria-selected', 'true');
  });

  it('shows an empty hint when no node is selected', () => {
    render(<DetailPanel node={null} record={null} findings={[]} episodePhase={null} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement**

```tsx
// webui/src/components/replay/detail/DetailPanel.tsx
import { useState } from 'react';
import type { FindingDto, GraphNodeDto } from '../../../api/types';
import { InsightTab } from './InsightTab';
import { DetailTab } from './DetailTab';
import { RawTab } from './RawTab';
import styles from './DetailPanel.module.css';

type TabId = 'insight' | 'detail' | 'raw';

interface DetailPanelProps {
  node: GraphNodeDto | null;
  record: unknown;
  findings: FindingDto[];
  episodePhase: string | null;
}

export function DetailPanel({ node, record, findings, episodePhase }: DetailPanelProps) {
  // null = "follow the default rule"; a value = an explicit user choice that sticks.
  const [chosen, setChosen] = useState<TabId | null>(null);
  const fallback: TabId = findings.length > 0 ? 'insight' : 'detail';
  const active = chosen ?? fallback;

  if (!node) {
    return <aside className={styles.panel}><p className={styles.empty}>Select a node to inspect it.</p></aside>;
  }

  const tab = (id: TabId, label: string) => (
    <button
      type="button"
      role="tab"
      aria-selected={active === id}
      className={`${styles.tab} ${active === id ? styles.active : ''}`}
      onClick={() => setChosen(id)}
    >
      {label}
    </button>
  );

  return (
    <aside className={styles.panel}>
      <div className={styles.tabs} role="tablist">
        {tab('insight', 'Insight')}
        {tab('detail', 'Detail')}
        {tab('raw', 'Raw')}
      </div>
      <div className={styles.body} role="tabpanel">
        {active === 'insight' && <InsightTab findings={findings} />}
        {active === 'detail' && <DetailTab node={node} record={record} episodePhase={episodePhase} />}
        {active === 'raw' && <RawTab nodeId={node.node_id ?? null} record={record} />}
      </div>
    </aside>
  );
}
```

```css
/* webui/src/components/replay/detail/DetailPanel.module.css */
.panel { display: flex; flex-direction: column; height: 100%; border: 1px solid var(--witmcc-border, #1d212c); border-radius: 6px; background: var(--witmcc-surface-1, #0f1217); }
.empty { color: var(--witmcc-fg-subtle, #6a7180); padding: 16px; }
.tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--witmcc-border, #1d212c); padding: 6px 6px 0; }
.tab { background: none; border: none; color: var(--witmcc-fg-muted, #aab0bd); padding: 6px 12px; cursor: pointer; border-radius: 4px 4px 0 0; font: inherit; }
.tab.active { color: var(--witmcc-fg, #e6e8ee); background: var(--witmcc-surface-2, #161a23); border-bottom: 2px solid var(--witmcc-accent, #4f8cff); }
.body { padding: 10px; overflow: auto; flex: 1; min-height: 0; }
```

> `node.node_id` — confirm `GraphNodeDto` has `node_id` (it does per types.ts). Pass it as the RawTab persistence key.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/detail/DetailPanel.tsx webui/src/components/replay/detail/DetailPanel.module.css webui/src/components/replay/detail/__tests__/DetailPanel.test.tsx
git commit -m "webui(redesign-v2) R3: DetailPanel tab shell + persistence"
```

---

### Task 6: Wire DetailPanel into the detail slot; remove SourcePanel + WhyPanel (TDD-adjusted)

**Files:** Modify `webui/src/routes/SessionDetailPage.tsx`, `__tests__/SessionDetailPage.test.tsx`.

- [ ] **Step 1: Add the raw-record fetch hook for the selected node.** There is an existing `getEventRaw(eventId)` client + the `SourcePanel` did the fetch. Reuse the existing query hook if present (`webui/src/lib/queries.ts`); otherwise fetch in an effect mirroring SourcePanel. Add a `useEventRawQuery(selectedEventId)` to `queries.ts` if not present:

```ts
// add to webui/src/lib/queries.ts (mirror existing query patterns there)
import { getEventRaw } from '../api/client';
export function useEventRawQuery(eventId: string | null) {
  return useQuery({
    queryKey: ['eventRaw', eventId],
    queryFn: () => getEventRaw(eventId as string),
    enabled: !!eventId,
    staleTime: 60_000,
  });
}
```
(Match the actual `useQuery` import/signature used by the other hooks in that file — read it first.)

- [ ] **Step 2: In SessionDetailPage**, compute the findings for the selected node and the raw record, then render `<DetailPanel>` in the detail slot. Add imports:
```tsx
import { DetailPanel } from '../components/replay/detail/DetailPanel';
import { useEventRawQuery } from '../lib/queries';
```
Add derived values (near the other useMemo blocks):
```tsx
  const rawQuery = useEventRawQuery(selectedEventId);
  const selectedNodeFindings = useMemo(() => {
    if (!sel.selectedNodeId) return [];
    const nid = sel.selectedNodeId;
    const node = effectiveGraph.nodes.find((n) => n.node_id === nid);
    const sourceEventIds = new Set(node?.source_event_ids ?? []);
    return findingsData.filter((f) =>
      f.evidence_refs.some((ref) => {
        if (typeof ref === 'string') return ref === nid || sourceEventIds.has(ref);
        return ref.node_id === nid || (typeof ref.event_id === 'string' && sourceEventIds.has(ref.event_id));
      }),
    );
  }, [sel.selectedNodeId, effectiveGraph, findingsData]);

  const selectedNodePhase = useMemo(() => {
    if (!selectedNode) return null;
    const eps = episodes.data ?? [];
    const t = selectedNode.started_at;
    return eps.find((e) => e.started_at <= t && t <= e.ended_at)?.phase ?? null;
  }, [selectedNode, episodes.data]);
```
Replace the detail slot body (the `<SourcePanel .../>`) with:
```tsx
            <DetailPanel
              node={selectedNode}
              record={rawQuery.data?.record ?? null}
              findings={selectedNodeFindings}
              episodePhase={selectedNodePhase}
            />
```
> Confirm the raw response shape: `getEventRaw` returns `RawEventResponse` with a `.record` field (SourcePanel used `state.data.record`). Pass `rawQuery.data?.record`. If the DTO wraps under `.data`, unwrap accordingly (check `RawEventResponse` in types.ts + how SourcePanel read it).

- [ ] **Step 3: Remove the `WhyPanel` mount** and its now-unused props/imports (`WhyPanel`, `evidenceQuery`, `selectedFinding` if unused elsewhere, `sel.whyPanelOpen`/`closeWhyPanel` if no longer used). Remove the `SourcePanel` import. Keep selection wiring intact. Run `tsc` to surface unused symbols and clean them.

- [ ] **Step 4: Update SessionDetailPage tests** — any assertion referencing SourcePanel's "Click a node to see its source record" or WhyPanel must move to the DetailPanel equivalents (e.g., clicking a node now shows the Detail/Insight tabs; the empty state is "Select a node to inspect it."). Add an assertion that the detail slot contains a `tablist` with Insight/Detail/Raw once a node is selected.

- [ ] **Step 5: Full suite + build**

Run: `cd webui && npx vitest run && npx tsc --noEmit && npm run build`
Expected: green. The old `SourcePanel.test.tsx` and `WhyPanel*.test.tsx`: if SourcePanel/WhyPanel are now unused everywhere, either delete the components + tests (preferred — dead code) or leave the components if still imported. Verify with grep; delete if dead (mirror R1 Task 4 deletion discipline). DiffHunk/Hook/OTel structured sections in SourcePanel: their content is superseded by DetailTab + RawTab for now; deleting SourcePanel is acceptable per spec §9 (SourcePanel placement removed; its structured sections "migrate into DetailTab" — keep parity for diff_hunk by showing the tree, acceptable for R3).

- [ ] **Step 6: Commit**

```bash
git add -A webui/src/routes webui/src/lib/queries.ts
git commit -m "webui(redesign-v2) R3: mount DetailPanel, remove SourcePanel + WhyPanel drawer"
```

---

### Task 7: Browser smoke

- [ ] Rebuild (`cd webui && npm run build && cd .. && cargo build`), serve, navigate to a session **with findings** (e.g. one whose KPI shows RISK > 0).
- [ ] Click a stream card / timeline node. Confirm the right panel shows **Insight / Detail / Raw** tabs (no giant raw JSON by default).
- [ ] Insight tab lists findings (summary/category/severity/confidence) for nodes that have them; Detail tab shows kind/time/episode/token badge; Raw tab shows the JSON tree.
- [ ] **Expand a few nodes in Raw, then trigger a refresh** (navigate within the session / wait for an SSE tick) — the expanded paths must remain open (the #2 fix).
- [ ] Fix issues, re-run. When clean, R3 done.

---

## Self-Review

- **Spec coverage:** §4 Insight (findings; WhyPanel absorbed), Detail (fields + token badge — the R2 deferral), Raw (tree). #1 raw demoted to a tab; #2 fixed via controlled JsonTree + per-node persistence (locked by JsonTree + RawTab re-render tests). Default-Insight-fallback-Detail + tab persistence locked by DetailPanel tests.
- **Placeholder scan:** Every step has full code or a concrete read-then-adapt instruction (queries.ts hook, raw record unwrap) grounded in existing files. No TBD.
- **Type consistency:** `DetailPanel` props (`node`, `record`, `findings`, `episodePhase`) consistent across tests + impl + wire-in. `JsonTree` (`data`/`expanded`/`onToggle`), `RawTab` (`nodeId`/`record`), `DetailTab` (`node`/`record`/`episodePhase`), `InsightTab` (`findings`) all match between tests, impls, and DetailPanel usage. Token name `--witmcc-warning` flagged to verify against tokens.css.
- **Open risk:** the focused insight subgraph (spec §6) is NOT in R3 — it lands in R5 inside InsightTab. R3 InsightTab is the findings list only; this is intentional sequencing.
