# Per-event 태그 분류기 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 메시지 뷰 activity-run의 각 이벤트에 *실제 행위*를 나타내는 결정론적 per-event 태그(칩)를 렌더타임에 붙이고, 인식 못 한 Bash 패턴을 dev 패널로 노출해 태그 테이블 확장을 쉽게 한다.

**Architecture:** frontend-only, 렌더타임·로컬. 단일 소스 테이블 `eventTags.ts`(`tagForEvent`/`collectUntagged`). 태그는 `ActivityStack`가 렌더 시 이벤트별로 `tagForEvent` 호출(O(1), buildStreamModel 무변경). 패널은 세션 이벤트에 `collectUntagged`. 실측으로 회귀 없음 확인.

**Tech Stack:** TypeScript/React, Vite, Vitest, Biome.

**Branch:** `per-event-tag-classifier` (`episode-phase-removal` 위 스택). 스펙: `docs/superpowers/specs/2026-05-31-per-event-tag-classifier-design.md`.

---

### Task 1: `eventTags.ts` — 단일 소스 테이블 + tagForEvent + collectUntagged

**Files:**
- Create: `webui/src/components/replay/stream/eventTags.ts`
- Test: `webui/src/components/replay/stream/__tests__/eventTags.test.ts`

- [ ] **Step 1: 실패 테스트 작성 (실데이터 첫토큰 앵커)**

`eventTags.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import { tagForEvent, collectUntagged } from '../eventTags';
import type { ObservedEventDto } from '../../../../api/types';

const bash = (command: string): ObservedEventDto =>
  ({ event_id: command, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } } as unknown as ObservedEventDto);
const read = (file_path: string): ObservedEventDto =>
  ({ event_id: file_path, kind: 'tool_call', tool_name: 'Read', observed_at: '2026-05-31T00:00:00Z', payload: { input: { file_path } } } as unknown as ObservedEventDto);

describe('tagForEvent — Bash (real-data anchored tokens)', () => {
  it('tags read/search tools', () => {
    expect(tagForEvent(bash('grep -n foo src')).tag).toBe('search·read');
    expect(tagForEvent(bash('find . -name "*.rs"')).tag).toBe('search·read');
    expect(tagForEvent(bash('ls -la')).tag).toBe('search·read');
    expect(tagForEvent(bash('cat Cargo.toml')).tag).toBe('search·read');
  });
  it('splits git into read vs write by subcommand', () => {
    expect(tagForEvent(bash('git status')).tag).toBe('vcs-read');
    expect(tagForEvent(bash('git diff HEAD')).tag).toBe('vcs-read');
    expect(tagForEvent(bash('git commit -m x')).tag).toBe('vcs-write');
    expect(tagForEvent(bash('git push')).tag).toBe('vcs-write');
  });
  it('tags build/test and query/script', () => {
    expect(tagForEvent(bash('cargo test --all')).tag).toBe('build·test');
    expect(tagForEvent(bash('npm run dev')).tag).toBe('build·test');
    expect(tagForEvent(bash('sqlite3 db .tables')).tag).toBe('query·script');
    expect(tagForEvent(bash('python3 script.py')).tag).toBe('query·script');
  });
  it('marks rm/mv as destructive', () => {
    expect(tagForEvent(bash('rm -rf target')).tag).toBe('destructive');
    expect(tagForEvent(bash('mv a b')).tag).toBe('destructive');
  });
  it('treats compound/redirect commands as ambiguous (no tag, show command)', () => {
    expect(tagForEvent(bash('cd x && grep y')).disposition).toBe('ambiguous');
    expect(tagForEvent(bash('grep y > out.txt')).disposition).toBe('ambiguous');
    expect(tagForEvent(bash('grep a | grep b | wc -l')).disposition).toBe('ambiguous');
    expect(tagForEvent(bash('cd x && grep y')).tag).toBeNull();
  });
  it('treats shell-control tokens as control (no chip, not untagged)', () => {
    expect(tagForEvent(bash('cd /tmp')).disposition).toBe('control');
    expect(tagForEvent(bash('echo hi')).disposition).toBe('control');
  });
  it('marks unrecognized SIMPLE first-tokens as unmatched (panel candidates)', () => {
    expect(tagForEvent(bash('gh pr view')).disposition).toBe('unmatched');
    expect(tagForEvent(bash('frobnicate x')).disposition).toBe('unmatched');
  });
});

describe('tagForEvent — Read by extension', () => {
  it('classifies code/docs/config/data', () => {
    expect(tagForEvent(read('src/a.rs')).tag).toBe('code');
    expect(tagForEvent(read('webui/x.tsx')).tag).toBe('code');
    expect(tagForEvent(read('README.md')).tag).toBe('docs');
    expect(tagForEvent(read('Cargo.toml')).tag).toBe('config');
    expect(tagForEvent(read('data.json')).tag).toBe('data');
  });
});

describe('tagForEvent — other tools get no chip', () => {
  it('Edit/Agent → control disposition (no chip)', () => {
    const edit = { event_id: 'e', kind: 'tool_call', tool_name: 'Edit', observed_at: '2026-05-31T00:00:00Z', payload: {} } as unknown as ObservedEventDto;
    expect(tagForEvent(edit).disposition).toBe('control');
    expect(tagForEvent(edit).tag).toBeNull();
  });
});

describe('collectUntagged', () => {
  it('aggregates only unmatched simple Bash by first token with count + sample, excludes control/ambiguous, and drops a token once a rule is added', () => {
    const events = [bash('gh pr view 1'), bash('gh pr view 2'), bash('cd /tmp'), bash('cd x && grep y'), bash('grep z')];
    const rows = collectUntagged(events);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ token: 'gh', count: 2 });
    expect(rows[0].sample).toContain('gh pr view');
    expect(rows[0].hint).toContain("BASH_FIRST_TOKEN_TAGS");
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/eventTags.test.ts`
Expected: FAIL (`eventTags` 모듈 없음).

- [ ] **Step 3: 구현 `eventTags.ts`**

```ts
// webui/src/components/replay/stream/eventTags.ts
import type { ObservedEventDto } from '../../../api/types';

export type ReadTag = 'code' | 'docs' | 'config' | 'data';
export type BashTag = 'search·read' | 'vcs-read' | 'vcs-write' | 'build·test' | 'query·script' | 'destructive';
export type Tag = ReadTag | BashTag;
export type Disposition = 'tagged' | 'control' | 'ambiguous' | 'unmatched';
export interface TagResult { tag: Tag | null; disposition: Disposition; }

// ── single source of truth — add a key to extend ──────────────────────────
export const READ_EXT_TAGS: Record<string, ReadTag> = {
  rs: 'code', ts: 'code', tsx: 'code', js: 'code', jsx: 'code', css: 'code',
  md: 'docs', html: 'docs', txt: 'docs',
  toml: 'config', yaml: 'config', yml: 'config', ini: 'config',
  json: 'data', sql: 'data', jsonl: 'data', log: 'data', csv: 'data',
};
export const BASH_FIRST_TOKEN_TAGS: Record<string, BashTag> = {
  grep: 'search·read', rg: 'search·read', egrep: 'search·read', fgrep: 'search·read',
  find: 'search·read', ls: 'search·read', cat: 'search·read', head: 'search·read',
  tail: 'search·read', wc: 'search·read', jq: 'search·read', tree: 'search·read',
  which: 'search·read', file: 'search·read', stat: 'search·read', du: 'search·read', df: 'search·read',
  cargo: 'build·test', npm: 'build·test', npx: 'build·test', pnpm: 'build·test',
  yarn: 'build·test', make: 'build·test', tsc: 'build·test', vitest: 'build·test', go: 'build·test',
  sqlite3: 'query·script', python3: 'query·script', python: 'query·script',
  node: 'query·script', osascript: 'query·script', psql: 'query·script', ruby: 'query·script',
};
export const GIT_SUBCOMMAND_TAGS: Record<string, BashTag> = {
  status: 'vcs-read', log: 'vcs-read', diff: 'vcs-read', show: 'vcs-read',
  branch: 'vcs-read', blame: 'vcs-read', 'rev-parse': 'vcs-read', describe: 'vcs-read',
  add: 'vcs-write', commit: 'vcs-write', push: 'vcs-write', checkout: 'vcs-write',
  switch: 'vcs-write', stash: 'vcs-write', rm: 'vcs-write', reset: 'vcs-write',
  merge: 'vcs-write', rebase: 'vcs-write', fetch: 'vcs-write', pull: 'vcs-write', tag: 'vcs-write', clone: 'vcs-write',
};
export const DESTRUCTIVE_FIRST_TOKENS = new Set(['rm', 'mv', 'rmdir']);
export const CONTROL_TOKENS = new Set(['cd', 'echo', 'sleep', 'for', 'export', 'source', 'set', 'pgrep', 'kill', 'wait', 'true', ':']);
export const BASH_COMPOUND_MARKERS = ['&&', '||', ';', '|', '>', '>>', '<', '$(', '`'];

function ext(path: string): string {
  const base = path.split('/').pop() ?? '';
  const i = base.lastIndexOf('.');
  return i > 0 ? base.slice(i + 1).toLowerCase() : '';
}
function firstToken(cmd: string): string {
  const t = cmd.trim();
  const sp = t.indexOf(' ');
  return (sp > 0 ? t.slice(0, sp) : t).toLowerCase();
}

export function tagForEvent(e: ObservedEventDto): TagResult {
  const tool = e.tool_name;
  const input = ((e.payload as Record<string, unknown>)?.input ?? {}) as Record<string, unknown>;

  if (tool === 'Read') {
    const fp = typeof input.file_path === 'string' ? input.file_path : '';
    const tag = READ_EXT_TAGS[ext(fp)];
    return tag ? { tag, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
  }
  if (tool === 'Bash' || tool === 'bash') {
    const cmd = typeof input.command === 'string' ? input.command.trim() : '';
    if (!cmd) return { tag: null, disposition: 'control' };
    if (BASH_COMPOUND_MARKERS.some((m) => cmd.includes(m))) return { tag: null, disposition: 'ambiguous' };
    const tok = firstToken(cmd);
    if (DESTRUCTIVE_FIRST_TOKENS.has(tok)) return { tag: 'destructive', disposition: 'tagged' };
    if (tok === 'git') {
      const sub = firstToken(cmd.slice(3).trim());
      const t = GIT_SUBCOMMAND_TAGS[sub];
      return t ? { tag: t, disposition: 'tagged' } : { tag: null, disposition: 'unmatched' };
    }
    const t = BASH_FIRST_TOKEN_TAGS[tok];
    if (t) return { tag: t, disposition: 'tagged' };
    if (CONTROL_TOKENS.has(tok)) return { tag: null, disposition: 'control' };
    return { tag: null, disposition: 'unmatched' };
  }
  // every other tool: tool name is the label → no chip
  return { tag: null, disposition: 'control' };
}

export interface UntaggedRow { token: string; count: number; sample: string; hint: string; }

export function collectUntagged(events: ObservedEventDto[]): UntaggedRow[] {
  const byToken = new Map<string, { count: number; sample: string }>();
  for (const e of events) {
    if (tagForEvent(e).disposition !== 'unmatched') continue;
    const input = ((e.payload as Record<string, unknown>)?.input ?? {}) as Record<string, unknown>;
    const cmd = typeof input.command === 'string' ? input.command.trim() : (typeof input.file_path === 'string' ? input.file_path : '');
    const tok = firstToken(cmd);
    const cur = byToken.get(tok);
    if (cur) cur.count++;
    else byToken.set(tok, { count: 1, sample: cmd.slice(0, 80) });
  }
  return [...byToken.entries()]
    .map(([token, v]) => ({ token, count: v.count, sample: v.sample, hint: `add '${token}': '<tag>' to BASH_FIRST_TOKEN_TAGS in eventTags.ts` }))
    .sort((a, b) => b.count - a.count);
}
```

- [ ] **Step 4: 통과 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/eventTags.test.ts && npx tsc --noEmit`
Expected: PASS, tsc exit 0.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/eventTags.ts webui/src/components/replay/stream/__tests__/eventTags.test.ts
git commit -m "feat(webui): single-source per-event tag table + tagForEvent/collectUntagged"
```

---

### Task 2: ActivityStack 칩 렌더

**Files:**
- Modify: `webui/src/components/replay/stream/ActivityStack.tsx` (event row, ~84행), `ActivityStack.module.css`
- Test: `webui/src/components/replay/stream/__tests__/ActivityStack.test.tsx`

- [ ] **Step 1: 실패 테스트 추가**

`ActivityStack.test.tsx`에 (기존 파일에 추가):
```tsx
it('renders a tag chip for a tagged Bash event (search·read) and none for control', () => {
  const ev = (command: string, id: string) => ({ event_id: id, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } });
  const stack = { events: [ { event: ev('grep -n x', 'a'), result: null }, { event: ev('cd /tmp', 'b'), result: null } ] };
  render(<ActivityStack stack={stack as any} selectedEventId={'a'} onSelect={() => {}} />); // selected → expanded
  expect(screen.getByText('search·read')).toBeInTheDocument();
  // control event 'cd' produces no chip text
  expect(screen.queryByText('control')).toBeNull();
});
```

- [ ] **Step 2: 실패 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/ActivityStack.test.tsx`
Expected: FAIL (칩 없음).

- [ ] **Step 3: 구현 — 이벤트 행에 칩**

`ActivityStack.tsx`: import 추가 `import { tagForEvent } from './eventTags';`. event row(현재 84행 `itemPrimary` span 뒤)에 추가:
```tsx
                <span className={styles.itemPrimary}>{label.primary}</span>
                {(() => { const tr = tagForEvent(ae.event); return tr.disposition === 'tagged' && tr.tag
                  ? <span data-testid="event-tag-chip" className={styles.tagChip}>{tr.tag}</span> : null; })()}
```
`ActivityStack.module.css`에 `.tagChip` 추가(경량):
```css
.tagChip { font-size: 10px; padding: 0 5px; border-radius: 6px; background: var(--witmcc-surface-2, #2a2f3a); color: var(--witmcc-text-dim, #9aa4b2); white-space: nowrap; }
```

- [ ] **Step 4: 통과 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/ActivityStack.test.tsx && npx tsc --noEmit`
Expected: PASS, tsc 0.

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/replay/stream/ActivityStack.tsx webui/src/components/replay/stream/ActivityStack.module.css webui/src/components/replay/stream/__tests__/ActivityStack.test.tsx
git commit -m "feat(webui): render per-event tag chip in activity-run"
```

---

### Task 3: UntaggedBashPanel (숨김·토글·소형) + 세션 뷰 마운트

**Files:**
- Create: `webui/src/components/replay/stream/UntaggedBashPanel.tsx`, `UntaggedBashPanel.module.css`
- Test: `webui/src/components/replay/stream/__tests__/UntaggedBashPanel.test.tsx`
- Modify: `webui/src/routes/SessionDetailPage.tsx` (패널 마운트)

- [ ] **Step 1: 실패 테스트**

`UntaggedBashPanel.test.tsx`:
```tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { UntaggedBashPanel } from '../UntaggedBashPanel';

const bash = (command: string, id: string) => ({ event_id: id, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } });

describe('UntaggedBashPanel', () => {
  it('is hidden by default and toggles open to show unmatched tokens with count + hint', () => {
    const events = [bash('gh pr view', '1'), bash('gh pr list', '2'), bash('grep x', '3')] as any;
    render(<UntaggedBashPanel events={events} />);
    expect(screen.queryByTestId('untagged-list')).toBeNull(); // hidden
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    expect(screen.getByText(/gh/)).toBeInTheDocument();
    expect(screen.getByText(/2/)).toBeInTheDocument();
    expect(screen.getByText(/BASH_FIRST_TOKEN_TAGS/)).toBeInTheDocument();
    expect(screen.queryByText(/grep/)).toBeNull(); // matched → excluded
  });
  it('shows nothing-to-do when all matched', () => {
    render(<UntaggedBashPanel events={[bash('grep x', '1')] as any} />);
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    expect(screen.getByText(/all Bash patterns tagged/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/UntaggedBashPanel.test.tsx`
Expected: FAIL.

- [ ] **Step 3: 구현 패널**

```tsx
// webui/src/components/replay/stream/UntaggedBashPanel.tsx
import { useMemo, useState } from 'react';
import type { ObservedEventDto } from '../../../api/types';
import { collectUntagged } from './eventTags';
import styles from './UntaggedBashPanel.module.css';

export function UntaggedBashPanel({ events }: { events: ObservedEventDto[] }) {
  const [open, setOpen] = useState(false);
  const rows = useMemo(() => collectUntagged(events), [events]);
  return (
    <div className={styles.wrap}>
      <button data-testid="untagged-toggle" className={styles.toggle} onClick={() => setOpen((v) => !v)}>
        untagged Bash {rows.length > 0 ? `(${rows.length})` : ''}
      </button>
      {open && (
        <div data-testid="untagged-list" className={styles.list}>
          {rows.length === 0 ? (
            <div className={styles.empty}>all Bash patterns tagged</div>
          ) : rows.map((r) => (
            <div key={r.token} className={styles.row}>
              <code className={styles.token}>{r.token}</code>
              <span className={styles.count}>×{r.count}</span>
              <code className={styles.sample}>{r.sample}</code>
              <span className={styles.hint}>{r.hint}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
```
`UntaggedBashPanel.module.css` (소형·기본 숨김은 open state로 처리):
```css
.wrap { position: fixed; right: 8px; bottom: 8px; z-index: 50; font-size: 11px; }
.toggle { background: var(--witmcc-surface-2, #2a2f3a); color: var(--witmcc-text-dim, #9aa4b2); border: none; border-radius: 6px; padding: 2px 8px; cursor: pointer; opacity: 0.6; }
.toggle:hover { opacity: 1; }
.list { margin-top: 4px; max-height: 40vh; overflow: auto; background: var(--witmcc-surface-1, #1a1e26); border: 1px solid var(--witmcc-border, #333); border-radius: 6px; padding: 6px; width: min(560px, 80vw); }
.row { display: grid; grid-template-columns: auto auto 1fr; gap: 6px; align-items: center; padding: 2px 0; }
.token { color: var(--witmcc-text, #e6e6e6); } .count { color: var(--witmcc-text-dim, #9aa4b2); }
.sample { color: var(--witmcc-text-dim, #9aa4b2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.hint { grid-column: 1 / -1; color: var(--witmcc-accent, #7aa2ff); font-size: 10px; } .empty { color: var(--witmcc-text-dim, #9aa4b2); }
```

- [ ] **Step 4: SessionDetailPage 마운트**

`SessionDetailPage.tsx`: import `import { UntaggedBashPanel } from '../components/replay/stream/UntaggedBashPanel';`. 세션 이벤트 배열(스트림에 쓰는 `window_.events` 등 기존 변수)을 넘겨 페이지 루트에 `<UntaggedBashPanel events={window_.events} />` 마운트(fixed 위치라 레이아웃 영향 없음). 정확한 events 변수명은 파일에서 확인해 사용.

- [ ] **Step 5: 통과 확인 + commit**

Run: `cd webui && npx vitest run src/components/replay/stream/__tests__/UntaggedBashPanel.test.tsx && npx tsc --noEmit` → PASS.
```bash
git add webui/src/components/replay/stream/UntaggedBashPanel.tsx webui/src/components/replay/stream/UntaggedBashPanel.module.css webui/src/components/replay/stream/__tests__/UntaggedBashPanel.test.tsx webui/src/routes/SessionDetailPage.tsx
git commit -m "feat(webui): untagged-Bash dev panel (toggle, live from single source)"
```

---

### Task 4: 실측 (성능 회귀 없음 확인)

**Files:** (측정용 임시 계측 — 커밋 안 함)

- [ ] **Step 1: 계측**

`SessionDetailPage.tsx`의 메모이즈된 `buildStreamModel(...)` 호출을 임시로 감싼다:
```ts
const t0 = performance.now();
const model = buildStreamModel(window_.events, metricsByReq);
console.log('[perf] buildStreamModel+render-tag ms=', (performance.now() - t0).toFixed(2), 'events=', window_.events.length);
```
(태그는 ActivityStack 렌더 시 계산되므로, 추가로 ActivityStack 내 `tagForEvent` 합산을 보려면 React DevTools Profiler의 commit 시간을 사용.)

- [ ] **Step 2: baseline vs after 측정**

`git stash` 또는 `episode-phase-removal` 체크아웃으로 **태그 추가 전** 동일 세션(대형: 01fe9550 또는 653ea169, 그리고 2c5d9a5a)에서 콘솔 ms 기록 → baseline. 다시 본 브랜치에서 측정 → after. **Network 탭에서 신규 요청 0건** 확인.

- [ ] **Step 3: 합격 판정**

대형 세션에서 태그로 인한 추가분 **< ~5ms**, 신규 fetch **0**이면 합격. 회귀면 원인 분석(테이블 lookup은 O(1)이라 회귀 시 다른 원인). 측정값(before/after ms, requests=0)을 PR 본문에 기록. 계측 `console.log`는 제거(커밋 안 함).

---

### Task 5: 통합 검증 + 브라우저 smoke + PR

- [ ] **Step 1: 전체 회귀**

Run: `cd webui && npx vitest run && npx tsc --noEmit` → 전부 PASS, tsc 0. (`cargo`는 frontend-only 변경이라 영향 없음 — 생략 가능하나 안전상 `cargo build`만 확인.)

- [ ] **Step 2: 재빌드 + serve/vite 재시작**

webui dist 빌드 + (frontend-only라 cargo 변경 없음) serve는 기존 바이너리 유지 가능하나, 디스크 dist 갱신 위해 vite는 HMR로 충분. 기존 serve(:7878)/vite(:5173) 살아있으면 vite가 자동 반영.

- [ ] **Step 3: 브라우저 smoke (정적 세션 2c5d9a5a)**

(i) activity-run 펼치면 Bash 행에 `search·read`/`vcs-read` 등 **칩 표시**, `cd`/복합 명령엔 칩 없음·명령 표시. (ii) Read 행에 `code`/`docs` 칩. (iii) 우하단 **untagged 토글** 클릭 → 인식 못 한 토큰 목록(예 `gh`·`curl`) + 건수 + 힌트 표시, 기본 숨김. (iv) 콘솔 에러 0. 스크린샷 저장.

- [ ] **Step 4: PR 생성**

`gh pr create --base per-event-tag-classifier의 부모(episode-phase-removal)`. 본문: 태그 taxonomy·단일소스 확장·untagged 패널·**실측 결과(before/after ms, requests=0)**·smoke·정직한 plan-vs-done.

---

## Self-Review

**Spec coverage:** §2 taxonomy→Task1 테이블, §3 로직→Task1 tagForEvent, §4 단일소스→Task1, §5 칩→Task2, §6 패널→Task3, §7 실측→Task4, §8 파일/테스트→Task1-3 + Task5, §9 결정(렌더시점 계산)→Task2에서 ActivityStack 호출로 확정. 누락 없음.

**Placeholder scan:** 모든 코드 step에 완전 코드. Task3 Step4의 "events 변수명은 파일에서 확인"은 SessionDetailPage의 기존 스트림 events 변수 사용 지시(구현 시 확인) — 동작 지시이지 placeholder 아님.

**Type consistency:** `TagResult{tag,disposition}`, `Disposition` 4종, `UntaggedRow{token,count,sample,hint}`, `tagForEvent`/`collectUntagged` 시그니처가 Task1 정의 ↔ Task2/3 사용에서 일관.
