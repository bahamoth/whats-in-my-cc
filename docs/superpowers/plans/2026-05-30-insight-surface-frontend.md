# Insight Surface Frontend — Implementation Plan (Slice 7 of insight-surface-redesign)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current 6-tile `KpiStrip` (outcome · verification · episodes · risk · cost · latency) with a redesigned **insight strip** organized around the spec's five diagnostic questions (design spec §3, §5 "Direction A"). New cards: **컨텍스트 효율** (cache-hit % + 고정 컨텍스트 + drill, Q1/Q5), **토큰** (billed vs cache-read split, Q2), **검증** (guards by kind/status from verification-runs, Q4), **도구 실패(사용자)** (user-visible count + drill, Q1/Q2), **비용** (public-pricing 추정 badge, Q2). Each card carries a **provenance badge** (측정 / 혼합 / 추정 / 미수집·예정) and a `?` tooltip; clicking a card toggles an inline drill-down. The bottom `EpisodeStrip` phase bar stays. **Removed (spec §1/§5/§11 P1):** Risk score, Episodes count, Outcome 3-state, latency p95.

**Architecture:** A pure view-model builder (`insightCards.ts`) turns the already-existing query data (`useSessionUsageQuery`, `useVerificationRunsQuery`, `useFindingsQuery`) into a typed `InsightCardModel[]` with provenance + drill payloads. This keeps all derivation logic (cache-hit, billed/cache-read split, guard-kind grouping, cost 추정, user-visible-failure heuristic, graceful 미수집·예정 fallbacks) **pure and unit-testable** in jsdom — layout/CSS is not tested (spec §9). Two small reusable primitives — `ProvenanceBadge` and `InfoTip` (hover + click-to-close `?`) — are built and tested first. The `InsightStrip` component composes them and owns the click-to-expand state, mirroring the existing `ActivityStack` toggle pattern (`fireEvent.click(...-toggle)` + `data-testid` items). `SessionDetailPage` swaps `KpiStrip` for `InsightStrip` and stops computing `outcome` / `riskCount` for the strip.

**Tech Stack:** React + TypeScript + @tanstack/react-query, CSS Modules, design tokens (`webui/src/styles/tokens.css`). Tests: `npx vitest run` + `npx tsc -b` (run from `webui/`). Browser smoke (witmcc serve + claude-in-chrome) required before the final commit per CLAUDE.md — the controller performs the live smoke.

**Depends on (degrades gracefully when absent):** slice 1 (usage facet + `useSessionUsageQuery`) — **already landed** (`webui/src/lib/queries.ts:112`, `webui/src/api/types.ts:181`). slice 2 (verification detection rewrite adds `detection_basis`/`status_basis`) — **not yet landed**; the 검증 card reads only fields present today (`command_kind`, `status`) and degrades the badge to 혼합 with a stated limit. slice 5 (backend cost endpoint) — **not landed**; cost is derived client-side as 추정 from usage tokens × a public pricing table, never shown as billing. slice 6 (cross-session baseline endpoint) — **not landed**; the "vs median" delta is rendered only if a baseline prop is supplied, otherwise omitted (no error). **No new backend work in this slice.**

**Out of scope for this plan:** backend verification rewrite (slice 2), backend cost endpoint (slice 5), backend baseline endpoint (slice 6), tool_failure backend reframe (slice 3 — this slice approximates user-visible failures from findings client-side and badges it accordingly), episode-classifier fix (slice 4). The phase bar (`EpisodeStrip`) is unchanged.

---

## File structure

| File | Responsibility | Create/Modify |
|---|---|---|
| `webui/src/components/replay/insight-strip/provenance.ts` | `Provenance` union + `PROVENANCE_LABEL` map | Create |
| `webui/src/components/replay/insight-strip/ProvenanceBadge.tsx` | badge by `provenance` prop | Create |
| `webui/src/components/replay/insight-strip/ProvenanceBadge.module.css` | badge colours (tokens) | Create |
| `webui/src/components/replay/insight-strip/__tests__/ProvenanceBadge.test.tsx` | badge contract test | Create |
| `webui/src/components/replay/insight-strip/InfoTip.tsx` | `?` hover + click-to-close tooltip | Create |
| `webui/src/components/replay/insight-strip/InfoTip.module.css` | tooltip styling | Create |
| `webui/src/components/replay/insight-strip/__tests__/InfoTip.test.tsx` | open/close behaviour | Create |
| `webui/src/components/replay/insight-strip/insightCards.ts` | pure `buildInsightCards(inputs)` → `InsightCardModel[]` | Create |
| `webui/src/components/replay/insight-strip/__tests__/insightCards.test.ts` | pure derivation tests | Create |
| `webui/src/components/replay/insight-strip/InsightStrip.tsx` | strip: cards + expand state + EpisodeStrip slot caller | Create |
| `webui/src/components/replay/insight-strip/InsightStrip.module.css` | strip layout (tokens) | Create |
| `webui/src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx` | card present/absent, expand, badge, removed-tiles | Create |
| `webui/src/routes/SessionDetailPage.tsx` | swap `KpiStrip` → `InsightStrip`; drop strip-only derivations | Modify |
| `webui/src/components/replay/KpiStrip.tsx` | delete | Delete |
| `webui/src/components/replay/KpiStrip.module.css` | delete | Delete |
| `webui/src/components/replay/__tests__/KpiStrip.test.tsx` | delete | Delete |
| `docs/implementation-notes.html` | new § for the insight strip | Modify |

---

## Task 1: Provenance primitive + `ProvenanceBadge`

**Files:**
- Create: `webui/src/components/replay/insight-strip/provenance.ts`
- Create: `webui/src/components/replay/insight-strip/ProvenanceBadge.tsx`
- Create: `webui/src/components/replay/insight-strip/ProvenanceBadge.module.css`
- Create: `webui/src/components/replay/insight-strip/__tests__/ProvenanceBadge.test.tsx`

Provenance vocabulary is fixed by spec §2 P3: **측정** (directly computed) / **혼합** (tool-match + keyword guess) / **추정** (heuristic) / **미수집·예정** (data exists at source but parsing not yet implemented).

- [ ] **Step 1: Write the provenance vocabulary module**

Create `webui/src/components/replay/insight-strip/provenance.ts`:

```typescript
/**
 * insight-surface-redesign slice-7 — provenance vocabulary (design spec §2 P3).
 * Every surfaced value states how much to trust it. The long-form text lives in
 * each card's `?` tooltip; the badge shows the short Korean label.
 */
export type Provenance = 'measured' | 'mixed' | 'estimated' | 'uncollected';

export const PROVENANCE_LABEL: Record<Provenance, string> = {
  measured: '측정',
  mixed: '혼합',
  estimated: '추정',
  uncollected: '미수집·예정',
};
```

- [ ] **Step 2: Write the failing badge test**

Create `webui/src/components/replay/insight-strip/__tests__/ProvenanceBadge.test.tsx`:

```typescript
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ProvenanceBadge } from '../ProvenanceBadge';

describe('ProvenanceBadge', () => {
  it('renders the Korean label for each provenance', () => {
    const { rerender } = render(<ProvenanceBadge provenance="measured" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('측정');
    rerender(<ProvenanceBadge provenance="mixed" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('혼합');
    rerender(<ProvenanceBadge provenance="estimated" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('추정');
    rerender(<ProvenanceBadge provenance="uncollected" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('미수집·예정');
  });

  it('exposes the provenance via data-provenance for token-based colouring', () => {
    render(<ProvenanceBadge provenance="uncollected" />);
    expect(screen.getByTestId('provenance-badge').dataset.provenance).toBe('uncollected');
  });
});
```

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/ProvenanceBadge.test.tsx 2>&1 | tail -15`
Expected: FAIL — `../ProvenanceBadge` does not resolve.

- [ ] **Step 3: Implement the badge**

Create `webui/src/components/replay/insight-strip/ProvenanceBadge.tsx`:

```tsx
/**
 * slice-7 — provenance badge. A small inline pill that states the trust level
 * of the value next to it (design spec §2 P3, §5).
 */
import { type Provenance, PROVENANCE_LABEL } from './provenance';
import styles from './ProvenanceBadge.module.css';

export function ProvenanceBadge({ provenance }: { provenance: Provenance }) {
  return (
    <span
      className={styles.badge}
      data-testid="provenance-badge"
      data-provenance={provenance}
    >
      {PROVENANCE_LABEL[provenance]}
    </span>
  );
}
```

Create `webui/src/components/replay/insight-strip/ProvenanceBadge.module.css`:

```css
.badge {
  display: inline-block;
  font-size: 10px;
  line-height: 1.4;
  padding: 1px 6px;
  border-radius: 999px;
  border: 1px solid var(--witmcc-border-strong);
  color: var(--witmcc-fg-muted);
  white-space: nowrap;
}

.badge[data-provenance='measured'] {
  color: var(--witmcc-success);
  border-color: var(--witmcc-success);
}
.badge[data-provenance='mixed'] {
  color: var(--witmcc-info);
  border-color: var(--witmcc-info);
}
.badge[data-provenance='estimated'] {
  color: var(--witmcc-warning);
  border-color: var(--witmcc-warning);
}
.badge[data-provenance='uncollected'] {
  color: var(--witmcc-fg-subtle);
  border-color: var(--witmcc-border);
}
```

- [ ] **Step 4: Run test + types**

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/ProvenanceBadge.test.tsx 2>&1 | tail -15` → PASS (2 tests)
Run: `cd webui && npx tsc -b 2>&1 | tail -10` → clean

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/insight-strip/provenance.ts webui/src/components/replay/insight-strip/ProvenanceBadge.tsx webui/src/components/replay/insight-strip/ProvenanceBadge.module.css webui/src/components/replay/insight-strip/__tests__/ProvenanceBadge.test.tsx
git commit -m "feat(insight-strip): Provenance vocabulary + ProvenanceBadge"
```

---

## Task 2: `InfoTip` — `?` hover + click-to-close tooltip

**Files:**
- Create: `webui/src/components/replay/insight-strip/InfoTip.tsx`
- Create: `webui/src/components/replay/insight-strip/InfoTip.module.css`
- Create: `webui/src/components/replay/insight-strip/__tests__/InfoTip.test.tsx`

Behaviour (spec §5: "`?` hover/click tooltip"): the `?` trigger opens on hover (`mouseEnter`) and on click; click again (or `mouseLeave` when not click-pinned) closes it. We model two state bits — `hovered` and `pinned` — so a click pins it open and a second click un-pins (the spec's "hover + click-to-close").

- [ ] **Step 1: Write the failing test**

Create `webui/src/components/replay/insight-strip/__tests__/InfoTip.test.tsx`:

```typescript
import { describe, expect, it } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { InfoTip } from '../InfoTip';

describe('InfoTip', () => {
  it('is closed by default — no tooltip in the DOM', () => {
    render(<InfoTip label="cache" text="cache-hit explanation" />);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('opens on hover and shows the explanatory text', () => {
    render(<InfoTip label="cache" text="cache-hit explanation" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip')).toHaveTextContent('cache-hit explanation');
  });

  it('closes again on mouse leave when not click-pinned', () => {
    render(<InfoTip label="cache" text="explain" />);
    const t = screen.getByTestId('infotip-trigger');
    fireEvent.mouseEnter(t);
    fireEvent.mouseLeave(t);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('click pins it open; mouse leave then keeps it open; second click closes it', () => {
    render(<InfoTip label="cache" text="explain" />);
    const t = screen.getByTestId('infotip-trigger');
    fireEvent.click(t);
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    fireEvent.mouseLeave(t);
    expect(screen.getByRole('tooltip')).toBeInTheDocument(); // pinned
    fireEvent.click(t);
    expect(screen.queryByRole('tooltip')).toBeNull(); // unpinned + not hovered
  });

  it('the trigger does not bubble its click to an enclosing handler', () => {
    let outer = 0;
    render(
      <div onClick={() => { outer += 1; }}>
        <InfoTip label="cache" text="explain" />
      </div>,
    );
    fireEvent.click(screen.getByTestId('infotip-trigger'));
    expect(outer).toBe(0); // stopPropagation so opening the tip never expands the card
  });
});
```

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/InfoTip.test.tsx 2>&1 | tail -15`
Expected: FAIL — `../InfoTip` does not resolve.

- [ ] **Step 2: Implement `InfoTip`**

Create `webui/src/components/replay/insight-strip/InfoTip.tsx`:

```tsx
/**
 * slice-7 — `?` tooltip for a card's long-form provenance / explanation
 * (design spec §2 P3, §5). Opens on hover OR click; click pins it open so the
 * user can read it without keeping the pointer on the trigger; a second click
 * closes it. The trigger stops click propagation so it never toggles the
 * enclosing card's expand state.
 */
import { useState } from 'react';
import styles from './InfoTip.module.css';

interface InfoTipProps {
  /** Short subject (used for the aria-label, e.g. the card title). */
  label: string;
  /** Long-form explanation shown in the tooltip body. */
  text: string;
}

export function InfoTip({ label, text }: InfoTipProps) {
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const open = hovered || pinned;

  return (
    <span className={styles.wrap}>
      <button
        type="button"
        data-testid="infotip-trigger"
        className={styles.trigger}
        aria-label={`${label} 설명`}
        aria-expanded={open}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onClick={(e) => {
          e.stopPropagation();
          setPinned((p) => !p);
        }}
      >
        ?
      </button>
      {open && (
        <span role="tooltip" className={styles.bubble}>
          {text}
        </span>
      )}
    </span>
  );
}
```

Create `webui/src/components/replay/insight-strip/InfoTip.module.css`:

```css
.wrap {
  position: relative;
  display: inline-flex;
}

.trigger {
  width: 16px;
  height: 16px;
  border-radius: 999px;
  border: 1px solid var(--witmcc-border-strong);
  background: var(--witmcc-surface-3);
  color: var(--witmcc-fg-muted);
  font-size: 10px;
  line-height: 1;
  cursor: help;
  padding: 0;
}

.bubble {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 10;
  width: max-content;
  max-width: 280px;
  padding: 8px 10px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--witmcc-fg);
  background: var(--witmcc-surface-3);
  border: 1px solid var(--witmcc-border-strong);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
}
```

- [ ] **Step 3: Run test + types**

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/InfoTip.test.tsx 2>&1 | tail -15` → PASS (5 tests)
Run: `cd webui && npx tsc -b 2>&1 | tail -10` → clean

- [ ] **Step 4: Commit**

```bash
git add webui/src/components/replay/insight-strip/InfoTip.tsx webui/src/components/replay/insight-strip/InfoTip.module.css webui/src/components/replay/insight-strip/__tests__/InfoTip.test.tsx
git commit -m "feat(insight-strip): InfoTip ? tooltip (hover + click-to-pin)"
```

---

## Task 3: `buildInsightCards` — pure view-model builder

**Files:**
- Create: `webui/src/components/replay/insight-strip/insightCards.ts`
- Create: `webui/src/components/replay/insight-strip/__tests__/insightCards.test.ts`

This is the heart of the slice and the only place real derivation logic lives, so it is fully unit-tested in jsdom (no DOM). It maps the existing query DTOs to a typed `InsightCardModel[]`. Real anchors:
- `SessionUsageDto` shape is fixed at `webui/src/api/types.ts:181` (`billed_tokens`, `cache_read_input_tokens`, `cache_hit_ratio`, `by_model`).
- `VerificationRunDto.command_kind` values come from the backend allowlist `classify()` at `src/insight/verification_allowlist.rs:43-79`: `test_suite_js`, `test_suite_rust`, `test_suite_py`, `test_suite_go`, `test_suite_java`, `build`, `build_check`, `lint`, `format_check`. We group these into guard kinds **test / build / lint / format**. `status` is one of `'passed' | 'failed' | 'skipped' | string` (`webui/src/api/types.ts:145`); slice-2's `detection_basis`/`status_basis` are **not present yet**, so 검증 is badged `mixed` (혼합) per spec §3 Q4.
- `FindingDto.category` / `severity` (`webui/src/api/types.ts:95-108`). Slice-3's backend split of user-visible vs internal `StructuredOutput` retries is not done; this slice approximates "user-visible tool failures" as findings whose `category` is in a `tool_failure`-like set and badges the card `estimated` (추정) with an honest tooltip. When findings are absent it shows `미수집·예정`.

Cost (spec §6.5 / §11.3): a public-pricing 추정 from usage tokens. We keep a tiny static price table keyed by a normalized model family and compute USD; always badged `estimated`. When usage is absent → `미수집·예정`.

- [ ] **Step 1: Write the failing test**

Create `webui/src/components/replay/insight-strip/__tests__/insightCards.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { buildInsightCards, type InsightInputs } from '../insightCards';
import type { SessionUsageDto, VerificationRunDto, FindingDto } from '../../../../api/types';

const usage: SessionUsageDto = {
  session_id: 's1',
  turns: 5,
  input_tokens: 200_000,
  cache_creation_input_tokens: 3_900_000,
  cache_read_input_tokens: 199_500_000,
  output_tokens: 1_300_000,
  billed_tokens: 5_400_000,
  cache_hit_ratio: 0.98,
  by_model: [
    { model: 'claude-opus-4-8', turns: 3, output_tokens: 1_000_000 },
    { model: 'claude-haiku-4-5-20251001', turns: 2, output_tokens: 300_000 },
  ],
};

function vr(kind: string, status: string): VerificationRunDto {
  return {
    verification_run_id: `vr_${kind}_${status}`,
    schema_version: '1',
    session_id: 's1',
    source: 'transcript_bash',
    command: kind,
    command_kind: kind,
    trigger_event_id: 'e',
    trigger_tool_use_id: null,
    status,
    started_at: '2026-05-30T00:00:00Z',
    ended_at: null,
    exit_code: null,
    failure_summary: null,
    covered_diff_hunk_ids: [],
  };
}

function finding(category: string, severity: FindingDto['severity']): FindingDto {
  return {
    finding_id: `f_${category}`,
    schema_version: '1',
    session_id: 's1',
    category,
    severity,
    confidence: 0.8,
    summary: 's',
    evidence_refs: ['n1'],
    evidence_projection: {},
    provenance: {},
    status: 'open',
    created_at: '',
  };
}

const EMPTY: InsightInputs = {
  usage: undefined,
  verificationRuns: undefined,
  findings: undefined,
};

function byId(inputs: InsightInputs) {
  return new Map(buildInsightCards(inputs).map((c) => [c.id, c]));
}

describe('buildInsightCards — card set', () => {
  it('emits exactly the five redesigned cards in order, dropping the old tiles', () => {
    const ids = buildInsightCards(EMPTY).map((c) => c.id);
    expect(ids).toEqual(['context', 'tokens', 'verification', 'tool_failure', 'cost']);
    // The removed tiles never appear (spec §1/§5/§11 P1).
    expect(ids).not.toContain('outcome');
    expect(ids).not.toContain('risk');
    expect(ids).not.toContain('episodes');
    expect(ids).not.toContain('latency');
  });
});

describe('buildInsightCards — context efficiency (Q1/Q5)', () => {
  it('shows cache-hit % (측정) from usage', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.value).toBe('98%');
    expect(c.provenance).toBe('measured');
  });

  it('falls back to 미수집·예정 when usage is absent', () => {
    const c = byId(EMPTY).get('context')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — tokens (Q2)', () => {
  it('shows billed and cache-read SEPARATELY, never summed (spec §3 Q2)', () => {
    const c = byId({ ...EMPTY, usage }).get('tokens')!;
    // billed 5.4M vs cache-read 199.5M shown as distinct facts
    expect(c.value).toBe('청구 5.4M');
    expect(c.detail).toContain('199.5M');
    expect(c.provenance).toBe('measured');
  });
  it('is 미수집·예정 with placeholder when usage absent', () => {
    const c = byId(EMPTY).get('tokens')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — verification guards (Q4)', () => {
  it('groups command_kind into test/build/lint/format with pass counts', () => {
    const runs = [
      vr('test_suite_rust', 'passed'),
      vr('test_suite_js', 'failed'),
      vr('build', 'passed'),
      vr('lint', 'passed'),
      vr('format_check', 'skipped'),
    ];
    const c = byId({ ...EMPTY, verificationRuns: runs }).get('verification')!;
    // 5 guards, 3 passed
    expect(c.value).toBe('가드 5 · 통과 3');
    // detection_basis not in the data yet → mixed (혼합) per spec §3 Q4
    expect(c.provenance).toBe('mixed');
    // by-kind breakdown is carried for the drill
    expect(c.drill?.byKind).toEqual({ test: 2, build: 1, lint: 1, format: 1 });
  });
  it('is 미수집·예정 when there are no verification runs', () => {
    const c = byId({ ...EMPTY, verificationRuns: [] }).get('verification')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — user-visible tool failures (Q1/Q2)', () => {
  it('counts tool_failure-category findings and badges 추정', () => {
    const fs = [
      finding('tool_failure', 'high'),
      finding('risky_action', 'high'), // not a tool failure → excluded
      finding('tool_failure', 'medium'),
    ];
    const c = byId({ ...EMPTY, findings: fs }).get('tool_failure')!;
    expect(c.value).toBe('2');
    expect(c.provenance).toBe('estimated');
  });
  it('is 미수집·예정 when findings are absent (not loaded yet)', () => {
    const c = byId(EMPTY).get('tool_failure')!;
    expect(c.provenance).toBe('uncollected');
  });
  it('shows 0 (측정-of-absence is still 추정 here) when findings loaded but none match', () => {
    const c = byId({ ...EMPTY, findings: [finding('context_bloat', 'low')] }).get('tool_failure')!;
    expect(c.value).toBe('0');
    expect(c.provenance).toBe('estimated');
  });
});

describe('buildInsightCards — cost (Q2, 추정)', () => {
  it('derives a public-pricing dollar estimate from usage tokens, badged 추정', () => {
    const c = byId({ ...EMPTY, usage }).get('cost')!;
    expect(c.provenance).toBe('estimated');
    // value is a formatted dollar string, never "billing"
    expect(c.value).toMatch(/^\$/);
  });
  it('is 미수집·예정 when usage absent', () => {
    const c = byId(EMPTY).get('cost')!;
    expect(c.provenance).toBe('uncollected');
    expect(c.value).toBe('—');
  });
});

describe('buildInsightCards — baseline delta (slice 6, optional)', () => {
  it('attaches a vs-median delta to the context card when a baseline is supplied', () => {
    const c = byId({ ...EMPTY, usage, baseline: { cache_hit_ratio: 0.9 } }).get('context')!;
    expect(c.baselineDelta).toBeDefined();
  });
  it('omits the delta gracefully when no baseline is supplied', () => {
    const c = byId({ ...EMPTY, usage }).get('context')!;
    expect(c.baselineDelta).toBeUndefined();
  });
});
```

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/insightCards.test.ts 2>&1 | tail -20`
Expected: FAIL — `../insightCards` does not resolve.

- [ ] **Step 2: Implement `insightCards.ts`**

Create `webui/src/components/replay/insight-strip/insightCards.ts`:

```typescript
/**
 * slice-7 — pure view-model builder for the insight strip (design spec §3/§5).
 * Turns the already-fetched query DTOs into typed cards with provenance and
 * drill payloads. ALL derivation logic lives here so it is unit-testable in
 * jsdom; the component only renders. Degrades gracefully: when a backend slice
 * has not landed, the relevant card is badged `uncollected` (미수집·예정).
 */
import type { SessionUsageDto, VerificationRunDto, FindingDto } from '../../../api/types';
import { formatPct, formatTokens, formatUsd } from '../../../lib/format';
import type { Provenance } from './provenance';

/** Optional cross-session baseline (slice 6). Absent today → no delta shown. */
export interface InsightBaseline {
  cache_hit_ratio?: number | null;
}

export interface InsightInputs {
  usage: SessionUsageDto | undefined;
  verificationRuns: VerificationRunDto[] | undefined;
  findings: FindingDto[] | undefined;
  baseline?: InsightBaseline;
}

export type InsightCardId =
  | 'context'
  | 'tokens'
  | 'verification'
  | 'tool_failure'
  | 'cost';

export interface InsightCardModel {
  id: InsightCardId;
  /** Korean card title shown in the strip. */
  title: string;
  /** Headline value, already formatted; `—` when uncollected. */
  value: string;
  /** One-line micro-detail under the value. */
  detail: string;
  provenance: Provenance;
  /** Long-form text for the `?` tooltip. */
  tooltip: string;
  /** Inline drill content shown when the card is expanded. */
  drill?: {
    lines: string[];
    byKind?: Record<string, number>;
  };
  /** Optional "vs your median" delta (slice 6); undefined when no baseline. */
  baselineDelta?: string;
}

const GUARD_KIND: Record<string, 'test' | 'build' | 'lint' | 'format'> = {
  test_suite_js: 'test',
  test_suite_rust: 'test',
  test_suite_py: 'test',
  test_suite_go: 'test',
  test_suite_java: 'test',
  build: 'build',
  build_check: 'build',
  lint: 'lint',
  format_check: 'format',
};

/** Findings categories that represent a user-visible tool failure (slice-3
 *  backend reframe not done yet, so this is an estimate). */
const TOOL_FAILURE_CATEGORIES = new Set(['tool_failure', 'failed_tool_call']);

/** Public per-1M-token USD prices (input-equivalent) by model family. Interim
 *  推定 only (spec §6.5/§11.3) — never presented as billing. */
const PRICE_PER_MTOK: Record<string, { input: number; output: number }> = {
  opus: { input: 15, output: 75 },
  sonnet: { input: 3, output: 15 },
  haiku: { input: 0.8, output: 4 },
};

function priceFamily(model: string): keyof typeof PRICE_PER_MTOK | null {
  const m = model.toLowerCase();
  if (m.includes('opus')) return 'opus';
  if (m.includes('sonnet')) return 'sonnet';
  if (m.includes('haiku')) return 'haiku';
  return null;
}

function estimateCostUsd(usage: SessionUsageDto): number | null {
  if (usage.by_model.length === 0) return null;
  // Billed input-equivalent = input + cache_creation; cache_read is free.
  // Attribute billed input by per-model output share (a deliberate 추정).
  const totalOutput = usage.by_model.reduce((a, m) => a + m.output_tokens, 0) || 1;
  const billedInput = usage.input_tokens + usage.cache_creation_input_tokens;
  let usd = 0;
  for (const m of usage.by_model) {
    const fam = priceFamily(m.model);
    if (!fam) continue;
    const price = PRICE_PER_MTOK[fam];
    const inputShare = (m.output_tokens / totalOutput) * billedInput;
    usd += (inputShare / 1_000_000) * price.input;
    usd += (m.output_tokens / 1_000_000) * price.output;
  }
  return usd;
}

function contextCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '캐시 적중률 = cache_read / (cache_read + cache_creation + input). 측정값(usage facet). ' +
    '고정 캐시 컨텍스트 크기·증가·캐시 미스는 펼쳐서 확인. 시스템 프롬프트/스킬/메모리 단위 분해와 ' +
    '"오염" 판정은 데이터에 없어 제공하지 않습니다(설계 §8 한계).';
  if (!inputs.usage) {
    return {
      id: 'context', title: '컨텍스트 효율', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const card: InsightCardModel = {
    id: 'context', title: '컨텍스트 효율',
    value: formatPct(u.cache_hit_ratio),
    detail: `캐시 읽기 ${formatTokens(u.cache_read_input_tokens)}`,
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `캐시 적중률 ${formatPct(u.cache_hit_ratio)}`,
        `캐시 읽기(무료) ${formatTokens(u.cache_read_input_tokens)}`,
        `캐시 생성 ${formatTokens(u.cache_creation_input_tokens)}`,
        `턴 수 ${u.turns}`,
      ],
    },
  };
  const base = inputs.baseline?.cache_hit_ratio;
  if (typeof base === 'number' && typeof u.cache_hit_ratio === 'number') {
    const d = Math.round((u.cache_hit_ratio - base) * 100);
    card.baselineDelta = `${d >= 0 ? '+' : ''}${d}%p vs 중앙값`;
  }
  return card;
}

function tokensCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '청구 토큰(input + cache_creation + output)과 캐시 읽기(무료)는 의미가 달라 절대 합산하지 않습니다 ' +
    '(설계 §3 Q2). 측정값(usage facet).';
  if (!inputs.usage) {
    return {
      id: 'tokens', title: '토큰', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  return {
    id: 'tokens', title: '토큰',
    value: `청구 ${formatTokens(u.billed_tokens)}`,
    detail: `캐시 읽기 ${formatTokens(u.cache_read_input_tokens)} (무료)`,
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `input ${formatTokens(u.input_tokens)}`,
        `cache_creation ${formatTokens(u.cache_creation_input_tokens)}`,
        `output ${formatTokens(u.output_tokens)}`,
        ...u.by_model.map((m) => `${m.model}: ${m.turns}턴 · 출력 ${formatTokens(m.output_tokens)}`),
      ],
    },
  };
}

function verificationCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '가드 = 실행된 테스트/빌드/린트/포맷 검사. 도구 매칭은 측정, 키워드 추정은 보조 — 현재 백엔드가 ' +
    'detection_basis를 제공하지 않아 혼합으로 표시. 파이프(|) 명령의 종료코드는 가려질 수 있어 통과 여부는 ' +
    '보수적으로 집계합니다. 브라우저 스모크/서브에이전트 테스트는 감지하지 않습니다(설계 §3 Q4 한계).';
  const runs = inputs.verificationRuns;
  if (!runs || runs.length === 0) {
    return {
      id: 'verification', title: '검증', value: '—',
      detail: runs ? '감지된 가드 없음' : '로딩 중',
      provenance: 'uncollected', tooltip: tip,
    };
  }
  const byKind: Record<string, number> = {};
  let passed = 0;
  for (const r of runs) {
    const k = GUARD_KIND[r.command_kind] ?? 'test';
    byKind[k] = (byKind[k] ?? 0) + 1;
    if (r.status === 'passed') passed += 1;
  }
  return {
    id: 'verification', title: '검증',
    value: `가드 ${runs.length} · 통과 ${passed}`,
    detail: Object.entries(byKind).map(([k, n]) => `${k} ${n}`).join(' · '),
    provenance: 'mixed', tooltip: tip,
    drill: {
      lines: runs.map((r) => `${r.command_kind} → ${r.status}`),
      byKind,
    },
  };
}

function toolFailureCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '사용자에게 보였던 도구 실패 추정치(설계 §3 Q1/Q2). 내부 StructuredOutput 재시도 등 잡음은 백엔드 ' +
    'reframe(슬라이스 3) 전까지 완전히 분리되지 않아 추정으로 표시. 펼치면 해당 finding으로 이동합니다.';
  const fs = inputs.findings;
  if (!fs) {
    return {
      id: 'tool_failure', title: '도구 실패(사용자)', value: '—',
      detail: '로딩 중', provenance: 'uncollected', tooltip: tip,
    };
  }
  const failures = fs.filter((f) => TOOL_FAILURE_CATEGORIES.has(f.category));
  return {
    id: 'tool_failure', title: '도구 실패(사용자)',
    value: `${failures.length}`,
    detail: failures.length === 0 ? '사용자 표시 실패 없음' : '펼쳐서 증거 확인',
    provenance: 'estimated', tooltip: tip,
    drill: { lines: failures.map((f) => `${f.severity} · ${f.summary}`) },
  };
}

function costCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '공개 가격표 × usage 토큰으로 계산한 추정치이며 실제 청구액이 아닙니다(설계 §6.5/§11.3). ' +
    'OTel claude_code.cost.usage 메트릭이 들어오면 대체됩니다. cache_read(무료)는 비용에서 제외.';
  if (!inputs.usage) {
    return {
      id: 'cost', title: '비용', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const usd = estimateCostUsd(inputs.usage);
  return {
    id: 'cost', title: '비용',
    value: `≈ ${formatUsd(usd ?? undefined)}`,
    detail: '공개 가격표 추정',
    provenance: 'estimated', tooltip: tip,
  };
}

export function buildInsightCards(inputs: InsightInputs): InsightCardModel[] {
  return [
    contextCard(inputs),
    tokensCard(inputs),
    verificationCard(inputs),
    toolFailureCard(inputs),
    costCard(inputs),
  ];
}
```

> **Note on `value: '≈ ${formatUsd(...)}'`** — the cost test asserts `c.value).toMatch(/^\$/)`. The `≈ ` prefix breaks that. Fix the test expectation OR drop the prefix; this plan **drops the prefix** to keep `formatUsd` output verbatim. Change the cost card `value` to `formatUsd(usd ?? undefined)` (no `≈ `) and put the "≈ 추정" wording in `detail`/badge. Make this edit before running the test.

- [ ] **Step 2a: Align cost value with the test**

In `costCard`, set:
```typescript
    value: formatUsd(usd ?? undefined),
    detail: '공개 가격표 추정 (≈)',
```

- [ ] **Step 3: Run test + types**

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/insightCards.test.ts 2>&1 | tail -25` → PASS (all cases)
Run: `cd webui && npx tsc -b 2>&1 | tail -10` → clean

- [ ] **Step 4: Commit**

```bash
git add webui/src/components/replay/insight-strip/insightCards.ts webui/src/components/replay/insight-strip/__tests__/insightCards.test.ts
git commit -m "feat(insight-strip): pure buildInsightCards view-model + tests"
```

---

## Task 4: `InsightStrip` component (compact strip + click-expand)

**Files:**
- Create: `webui/src/components/replay/insight-strip/InsightStrip.tsx`
- Create: `webui/src/components/replay/insight-strip/InsightStrip.module.css`
- Create: `webui/src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx`

Direction A (spec §5): a compact strip of cards; clicking a card toggles an inline detail panel in place. Each card shows title · value · micro-detail · `ProvenanceBadge` · `InfoTip`. Expand state is single-open (clicking another card moves the open panel) and mirrors the `ActivityStack` toggle pattern (a `*-toggle` button + per-card `data-testid`). The strip takes the same input props the page already has and calls `buildInsightCards` internally so the page wiring stays thin.

- [ ] **Step 1: Write the failing test**

Create `webui/src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx`:

```typescript
import { describe, expect, it } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { InsightStrip } from '../InsightStrip';
import type { SessionUsageDto, VerificationRunDto } from '../../../../api/types';

const usage: SessionUsageDto = {
  session_id: 's1', turns: 5, input_tokens: 200_000,
  cache_creation_input_tokens: 3_900_000, cache_read_input_tokens: 199_500_000,
  output_tokens: 1_300_000, billed_tokens: 5_400_000, cache_hit_ratio: 0.98,
  by_model: [{ model: 'claude-opus-4-8', turns: 5, output_tokens: 1_300_000 }],
};

function vr(kind: string, status: string): VerificationRunDto {
  return {
    verification_run_id: `vr_${kind}`, schema_version: '1', session_id: 's1',
    source: 'transcript_bash', command: kind, command_kind: kind,
    trigger_event_id: 'e', trigger_tool_use_id: null, status,
    started_at: '2026-05-30T00:00:00Z', ended_at: null, exit_code: null,
    failure_summary: null, covered_diff_hunk_ids: [],
  };
}

describe('InsightStrip', () => {
  it('renders the five redesigned cards and NONE of the removed tiles', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} findings={[]} />);
    expect(screen.getByTestId('insight-card-context')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-tokens')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-verification')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-tool_failure')).toBeInTheDocument();
    expect(screen.getByTestId('insight-card-cost')).toBeInTheDocument();
    // removed tiles (spec §1/§5/§11 P1)
    expect(screen.queryByTestId('kpi-risk')).toBeNull();
    expect(screen.queryByTestId('kpi-episodes')).toBeNull();
    expect(screen.queryByTestId('kpi-outcome')).toBeNull();
    expect(screen.queryByTestId('kpi-latency')).toBeNull();
  });

  it('shows the cache-hit value and a 측정 badge on the context card', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} findings={[]} />);
    const card = screen.getByTestId('insight-card-context');
    expect(within(card).getByText('98%')).toBeInTheDocument();
    expect(within(card).getByTestId('provenance-badge')).toHaveTextContent('측정');
  });

  it('badges 미수집·예정 when usage is absent (slice not yet wired)', () => {
    render(<InsightStrip usage={undefined} verificationRuns={[]} findings={[]} />);
    const card = screen.getByTestId('insight-card-context');
    expect(within(card).getByTestId('provenance-badge')).toHaveTextContent('미수집·예정');
  });

  it('expands a card on click to show its drill lines, and collapses on second click', () => {
    render(
      <InsightStrip usage={usage} verificationRuns={[vr('test_suite_rust', 'passed')]} findings={[]} />,
    );
    expect(screen.queryByTestId('insight-drill-verification')).toBeNull();
    fireEvent.click(screen.getByTestId('insight-card-verification-toggle'));
    const drill = screen.getByTestId('insight-drill-verification');
    expect(within(drill).getByText(/test_suite_rust → passed/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('insight-card-verification-toggle'));
    expect(screen.queryByTestId('insight-drill-verification')).toBeNull();
  });

  it('is single-open — expanding one card closes the previously open one', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} findings={[]} />);
    fireEvent.click(screen.getByTestId('insight-card-context-toggle'));
    expect(screen.getByTestId('insight-drill-context')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('insight-card-tokens-toggle'));
    expect(screen.queryByTestId('insight-drill-context')).toBeNull();
    expect(screen.getByTestId('insight-drill-tokens')).toBeInTheDocument();
  });

  it('clicking the ? tooltip does NOT expand the card', () => {
    render(<InsightStrip usage={usage} verificationRuns={[]} findings={[]} />);
    const card = screen.getByTestId('insight-card-context');
    fireEvent.click(within(card).getByTestId('infotip-trigger'));
    expect(screen.queryByTestId('insight-drill-context')).toBeNull();
    // the tooltip itself opened
    expect(within(card).getByRole('tooltip')).toBeInTheDocument();
  });
});
```

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx 2>&1 | tail -20`
Expected: FAIL — `../InsightStrip` does not resolve.

- [ ] **Step 2: Implement `InsightStrip`**

Create `webui/src/components/replay/insight-strip/InsightStrip.tsx`:

```tsx
/**
 * slice-7 — the redesigned insight surface (design spec §5 "Direction A").
 * Replaces the 6-tile KpiStrip. Compact cards organized around the five
 * diagnostic questions, each with a provenance badge and a `?` tooltip;
 * clicking a card toggles an inline drill-down (single-open). The removed
 * tiles (Risk / Episodes / Outcome / latency, spec §1/§5/§11 P1) are gone.
 *
 * All derivation is pure in `insightCards.ts`; this component only renders and
 * owns the expand state. The phase bar (EpisodeStrip) stays in the page below.
 */
import { useState } from 'react';
import type { SessionUsageDto, VerificationRunDto, FindingDto } from '../../../api/types';
import { buildInsightCards, type InsightBaseline, type InsightCardId } from './insightCards';
import { ProvenanceBadge } from './ProvenanceBadge';
import { InfoTip } from './InfoTip';
import styles from './InsightStrip.module.css';

interface InsightStripProps {
  usage: SessionUsageDto | undefined;
  verificationRuns: VerificationRunDto[] | undefined;
  findings: FindingDto[] | undefined;
  /** slice 6 — optional cross-session baseline; omitted today. */
  baseline?: InsightBaseline;
}

export function InsightStrip(props: InsightStripProps) {
  const cards = buildInsightCards({
    usage: props.usage,
    verificationRuns: props.verificationRuns,
    findings: props.findings,
    baseline: props.baseline,
  });
  const [openId, setOpenId] = useState<InsightCardId | null>(null);

  return (
    <section className={styles.strip} aria-label="세션 인사이트">
      <div className={styles.row}>
        {cards.map((card) => {
          const open = openId === card.id;
          return (
            <div
              key={card.id}
              className={styles.card}
              data-testid={`insight-card-${card.id}`}
              data-provenance={card.provenance}
              data-open={open}
            >
              <div className={styles.cardHead}>
                <span className={styles.cardTitle}>{card.title}</span>
                <InfoTip label={card.title} text={card.tooltip} />
              </div>
              <button
                type="button"
                className={styles.cardToggle}
                data-testid={`insight-card-${card.id}-toggle`}
                aria-expanded={open}
                onClick={() => setOpenId((cur) => (cur === card.id ? null : card.id))}
              >
                <span className={styles.cardValue}>{card.value}</span>
                <span className={styles.cardDetail}>{card.detail}</span>
              </button>
              <div className={styles.cardFoot}>
                <ProvenanceBadge provenance={card.provenance} />
                {card.baselineDelta && (
                  <span className={styles.baselineDelta}>{card.baselineDelta}</span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {cards.map((card) =>
        openId === card.id && card.drill ? (
          <div
            key={`drill-${card.id}`}
            className={styles.drill}
            data-testid={`insight-drill-${card.id}`}
          >
            <ul className={styles.drillList}>
              {card.drill.lines.map((line, i) => (
                <li key={i} className={styles.drillItem}>{line}</li>
              ))}
            </ul>
          </div>
        ) : null,
      )}
    </section>
  );
}
```

Create `webui/src/components/replay/insight-strip/InsightStrip.module.css`:

```css
.strip {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  background: var(--witmcc-surface-1);
  border-bottom: 1px solid var(--witmcc-border);
}

.row {
  display: grid;
  grid-template-columns: repeat(5, minmax(140px, 1fr));
  gap: 8px;
}

.card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  background: var(--witmcc-surface-2);
  border: 1px solid var(--witmcc-border);
  border-radius: 6px;
  min-width: 0;
}
.card[data-open='true'] { border-color: var(--witmcc-accent); }

.cardHead {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
}
.cardTitle {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--witmcc-fg-subtle);
}

.cardToggle {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  width: 100%;
  padding: 0;
  border: none;
  background: none;
  text-align: left;
  cursor: pointer;
}
.cardValue {
  font-size: 16px;
  font-weight: 600;
  color: var(--witmcc-fg);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
.cardDetail {
  font-size: 11px;
  color: var(--witmcc-fg-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.cardFoot {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.baselineDelta {
  font-size: 10px;
  color: var(--witmcc-fg-subtle);
}

.drill {
  padding: 10px 12px;
  background: var(--witmcc-surface-2);
  border: 1px solid var(--witmcc-border);
  border-radius: 6px;
}
.drillList { margin: 0; padding-left: 18px; }
.drillItem {
  font-size: 12px;
  color: var(--witmcc-fg-muted);
  line-height: 1.6;
}

@media (max-width: 1100px) {
  .row { grid-template-columns: repeat(3, minmax(120px, 1fr)); }
}
@media (max-width: 640px) {
  .row {
    grid-auto-flow: column;
    grid-auto-columns: minmax(160px, 1fr);
    grid-template-columns: none;
    overflow-x: auto;
  }
}
```

- [ ] **Step 3: Run test + types**

Run: `cd webui && npx vitest run src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx 2>&1 | tail -20` → PASS (6 tests)
Run: `cd webui && npx tsc -b 2>&1 | tail -10` → clean

- [ ] **Step 4: Commit**

```bash
git add webui/src/components/replay/insight-strip/InsightStrip.tsx webui/src/components/replay/insight-strip/InsightStrip.module.css webui/src/components/replay/insight-strip/__tests__/InsightStrip.test.tsx
git commit -m "feat(insight-strip): InsightStrip — five cards + click-expand drill"
```

---

## Task 5: Wire `InsightStrip` into `SessionDetailPage` + remove `KpiStrip`

**Files:**
- Modify: `webui/src/routes/SessionDetailPage.tsx`
- Delete: `webui/src/components/replay/KpiStrip.tsx`, `KpiStrip.module.css`, `__tests__/KpiStrip.test.tsx`

The page already fetches everything the strip needs: `useSessionUsageQuery` is **not yet called** here — add it next to the other hooks; `useVerificationRunsQuery` and `useFindingsQuery` are already wired (`SessionDetailPage.tsx:59-60`). After the swap, `outcome`, `riskCount`, and `verificationCoverage` are no longer needed *for the strip* — but check whether any survive for other consumers before deleting (grep). As of today they feed only `KpiStrip`, so they can go; `findingsData`, `findingEventIds`, and `phaseByEventId` stay (used by the stream).

- [ ] **Step 1: Add the usage query hook**

In `SessionDetailPage.tsx`, add `useSessionUsageQuery` to the import from `../lib/queries` (the block at lines 16-24) and call it next to the other queries (after `useVerificationRunsQuery` at line 60):

```tsx
  const usage = useSessionUsageQuery(sessionId);
```

- [ ] **Step 2: Swap the component import**

Replace the import line `import { KpiStrip } from '../components/replay/KpiStrip';` (line 9) with:

```tsx
import { InsightStrip } from '../components/replay/insight-strip/InsightStrip';
```

- [ ] **Step 3: Replace the render usage**

Replace the `<KpiStrip ... />` block (lines 233-238) with:

```tsx
            <InsightStrip
              usage={usage.data}
              verificationRuns={verificationRuns.data}
              findings={findings.data}
            />
```

(`EpisodeStrip` and `MetaStrip` directly below it are unchanged — the phase bar stays per spec §5.)

- [ ] **Step 4: Remove now-dead strip-only derivations**

First confirm they have no other consumer:

Run: `cd webui && grep -rn "riskCount\|verificationCoverage\|\boutcome\b" src/routes/SessionDetailPage.tsx`

If the only references are their own `useMemo` definitions (lines 92-110), delete the three `useMemo` blocks (`riskCount`, `verificationCoverage`, `outcome`) and the now-unused `KpiOutcome`/`KpiStrip` types. Keep `findingsData` (line 91 — used by `findingEventIds` and `selectedNodeFindings`). Do **not** remove `diffHunks` if other code uses it; grep first:

Run: `cd webui && grep -rn "diffHunks" src/routes/SessionDetailPage.tsx`
If `diffHunks` is referenced only by the deleted `verificationCoverage`, also remove the `useDiffHunksQuery` call + its import; otherwise leave it.

- [ ] **Step 5: Delete the old KpiStrip files**

```bash
git rm webui/src/components/replay/KpiStrip.tsx webui/src/components/replay/KpiStrip.module.css webui/src/components/replay/__tests__/KpiStrip.test.tsx
```

- [ ] **Step 6: Run the full frontend suite + types**

Run: `cd webui && npx tsc -b 2>&1 | tail -15` → clean (catches any dangling import/var from Step 4)
Run: `cd webui && npx vitest run 2>&1 | tail -25` → all green; the old KpiStrip test is gone, no other suite references it.

If `tsc` flags `formatPct`/`formatUsd`/`formatMs` as unused imports in `SessionDetailPage.tsx` (they were only used by the deleted derivations), remove them from that file's imports.

- [ ] **Step 7: Commit**

```bash
git add webui/src/routes/SessionDetailPage.tsx
git commit -m "feat(insight-strip): replace KpiStrip with InsightStrip in SessionDetailPage"
```

---

## Task 6: Build embedded dist + browser smoke + implementation notes

**Files:**
- Modify: `docs/implementation-notes.html`

Per CLAUDE.md the WebUI rule: `cargo build` + vitest passing is **not** completion — a live browser smoke is required before commit. The controller performs the live smoke; this task documents the steps and records the implementation notes.

- [ ] **Step 1: Rebuild the embedded dist**

The two-server dev setup (MEMORY.md "witmcc-webui-dev-preview"): `witmcc serve` serves the embedded `webui/dist`; Vite dev (`:5173`) is for hot iteration. To smoke against `witmcc serve`, rebuild the bundle first:

Run: `cd webui && npx vite build 2>&1 | tail -10`
Expected: build succeeds; `webui/dist` updated. Then (from repo root) `cargo build 2>&1 | tail -5` if the embed is compiled in.

- [ ] **Step 2: Live browser smoke (controller)**

Start the server and navigate to a real session that has usage data (the spec's anchor session `653ea169-1121-442e-9cc9-776471a10895`, which `init-db` + `ingest --all` from slice 1's Task 7 populates):

```
cargo run --bin witmcc -- serve --bind 127.0.0.1 --port 7878
```

Then with claude-in-chrome navigate to `http://127.0.0.1:7878/sessions/653ea169-1121-442e-9cc9-776471a10895` and verify:
- five cards render (컨텍스트 효율 / 토큰 / 검증 / 도구 실패(사용자) / 비용); Risk / Episodes / Outcome / Latency are gone.
- 컨텍스트 효율 shows a high cache-hit % with a **측정** badge; 토큰 shows billed vs cache-read **separately**; 비용 shows a `$…` value with a **추정** badge.
- clicking a card expands an inline drill; clicking again collapses it; the `?` opens a tooltip without expanding the card.
- the phase bar (EpisodeStrip) still renders below the cards.
- for a session lacking usage facet rows, the affected cards show **미수집·예정** rather than `NaN`/blank.

Stop the server afterward.

- [ ] **Step 3: Implementation notes**

Add a new `§` to `docs/implementation-notes.html`: the InsightStrip redesign (slice 7) — the five cards and their question mapping (§3), the provenance badge vocabulary (측정/혼합/추정/미수집·예정), the pure `buildInsightCards` derivation (cache-hit, billed/cache-read separation, guard-kind grouping from `command_kind`, client-side cost 추정 with the interim public price table, user-visible-failure heuristic), and the graceful-degradation contract (cards badge 미수집·예정 when the upstream slice — verification rewrite §6.2, cost endpoint §6.5, baseline §11.1 — has not landed). Note that no schema/migration change is in this slice (frontend-only).

```bash
git add docs/implementation-notes.html
git commit -m "docs(insight-strip): implementation notes for the redesigned insight surface"
```

---

## Done criteria

- The six-tile `KpiStrip` is gone (files deleted, no references); `InsightStrip` renders the five redesigned cards (컨텍스트 효율 · 토큰 · 검증 · 도구 실패(사용자) · 비용), each with a provenance badge and a `?` tooltip, with click-to-expand drill (single-open). The `EpisodeStrip` phase bar remains.
- Risk score, Episodes count, Outcome 3-state, and latency p95 are removed from the surface (spec §1/§5/§11 P1); a contract test asserts their tiles are absent.
- All derivation is pure and tested (`insightCards.test.ts`); badge rendering, tooltip open/close, expand/collapse, and removed-tile absence are tested as behaviour (jsdom — no layout/CSS assertions, per spec §9).
- Graceful degradation: cards badge **미수집·예정** when usage/verification/findings data is absent (so the slice ships before slices 2/5/6 land); cost is a client-side **추정**, never billing; the slice-6 baseline delta renders only when a baseline prop is supplied.
- `npx vitest run` + `npx tsc -b` (from `webui/`) clean; no regressions. Live browser smoke completed before the final commit (controller).
- Next: slice 2 (verification rewrite) upgrades the 검증 badge to read `detection_basis`/`status_basis`; slice 5 swaps the cost 추정 for the OTel metric when present; slice 6 supplies the baseline prop.
