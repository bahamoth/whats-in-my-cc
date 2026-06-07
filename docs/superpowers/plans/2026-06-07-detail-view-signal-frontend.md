# Detail View — Signal Frontend Transition (Plan 2a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 프론트엔드를 제거된 finding API에서 새 `/v1/signals` API로 전환한다 — 기능 동등(현 디테일 뷰 구조 유지)하게 만들어 깨진 통합선을 복구하고, signal에 맞게 표시한다(severity/confidence 없음 → detector/subkind/facts). **공통 골격 5층 재설계는 Plan 2b에서 별도.**

**Architecture:** `getSignals`/`SignalDto`/`useSignalsQuery`로 교체. `findingEventIds`→`signalEventIds`(evidence_refs 로직 동일). `InsightTab`의 `FindingsList`→`SignalsList`(severity/confidence 제거, detector·subkind 표시). 제거된 `getFindingEvidence`/`getToolFailureSummary` 및 그 타입/훅 삭제. `InsightStrip`의 tool_failure 카드는 signal(detector=tool_failure) 카운트로 최소 복구(KPI 전면 개편은 별도 트랙 — 메모리: defer KPI).

**Tech Stack:** TypeScript, React 18, @tanstack/react-query v5, Vitest, Vite.

**Spec/refs:** `docs/superpowers/specs/2026-06-07-detail-view-derived-metrics-design.md` §8.1·2; 백엔드 SignalDto는 `src/api/dto.rs`. 프론트 finding 사용 지점은 조사 체크리스트(이 plan File Structure 참고).

**Backend contract (이미 배포됨):**
- `GET /v1/sessions/:id/signals` → `{ data: SignalDto[] }` (Envelope `.data`)
- `GET /v1/signals/:id` → `{ data: SignalDto }`
- `SignalDto = { signal_id, schema_version, session_id, detector, subkind: string|null, summary, evidence_refs: EvidenceRef[], facts: object, provenance: object, created_at }` — **NO severity/confidence/category/status**
- 제거됨: `/v1/sessions/:id/findings`, `/v1/findings/:id/evidence`, `/v1/sessions/:id/tool-failures`

---

## File Structure

- Modify: `webui/src/api/types.ts` — add `SignalDto`; remove `FindingDto`, `ToolFailureSummaryDto`, `FindingEvidenceResponse`
- Modify: `webui/src/api/client.ts` — add `getSignals`; remove `getFindings`, `getToolFailureSummary`, `getFindingEvidence`
- Modify: `webui/src/lib/queries.ts` — add `signals` key + `useSignalsQuery`; remove finding/toolFailure/findingEvidence keys+hooks
- Modify: `webui/src/routes/SessionDetailPage.tsx` — `useSignalsQuery`, `signalEventIds`, `selectedNodeSignals`, InsightStrip `signals` prop
- Modify: `webui/src/components/replay/detail/DetailPanel.tsx` — `signals: SignalDto[]` prop
- Modify: `webui/src/components/replay/detail/InsightTab.tsx` — `SignalsList` (detector/subkind/summary)
- Modify: `webui/src/components/replay/detail/InsightTab.module.css` — detector/subkind 스타일(severity 스타일 재사용 or 신규)
- Modify: `webui/src/components/replay/insight-strip/InsightStrip.tsx` + `insightCards.ts` — `signals` 입력, tool_failure 카드 signal 카운트
- Modify tests: `webui/src/api/__tests__/client.endpoints.test.ts`, `components/replay/detail/__tests__/InsightTab.test.tsx`, `routes/__tests__/SessionDetailPage.test.tsx`, insight-strip 카드 테스트

---

## Task 1: types + client (signal in, finding out)

**Files:** `webui/src/api/types.ts`, `webui/src/api/client.ts`, `webui/src/api/__tests__/client.endpoints.test.ts`

- [ ] **Step 1: types.ts** — add `SignalDto`, remove finding types.

```typescript
export type SignalDto = {
  signal_id: string;
  schema_version: string;
  session_id: string;
  detector: string;
  subkind: string | null;
  summary: string;
  evidence_refs: EvidenceRef[];
  facts: Record<string, unknown>;
  provenance: Record<string, unknown>;
  created_at: string;
};
```
Delete `FindingDto`, `ToolFailureSummaryDto`, `FindingEvidenceResponse`. Keep `EvidenceRef`.

- [ ] **Step 2: client.ts** — add getSignals, remove finding fns.

```typescript
export const getSignals = (id: string): Promise<SignalDto[]> =>
  jsonGet<SignalDto[]>(`/v1/sessions/${encodeURIComponent(id)}/signals`);
```
Remove `getFindings`, `getToolFailureSummary`, `getFindingEvidence` and their imports.

- [ ] **Step 3: 테스트 갱신 (red first)** — `client.endpoints.test.ts`: replace the `getFindings`/`getFindingEvidence`/`getToolFailureSummary` describe blocks with a `getSignals` test:

```typescript
describe('getSignals', () => {
  it('hits GET /v1/sessions/:id/signals and unwraps `data`', async () => {
    const expected = [{ signal_id: 'sig1', detector: 'tool_failure', evidence_refs: ['ev1'], facts: {}, summary: 's' }];
    fetchSpy.mockImplementation(mockJson({ data: expected }));
    const out = await getSignals('SES-1');
    expect(fetchSpy).toHaveBeenCalledWith('/v1/sessions/SES-1/signals', expect.any(Object));
    expect(out).toEqual(expected);
  });
});
```
(Match the file's actual `mockJson`/`fetchSpy` helpers.)

- [ ] **Step 4: vitest** — `cd webui && pnpm vitest run src/api/__tests__/client.endpoints.test.ts` (use the repo's actual runner — check `package.json`; it lists `vitest`. If pnpm absent, use the project's package manager). Expected PASS.

- [ ] **Step 5: Commit**
```bash
git add webui/src/api/types.ts webui/src/api/client.ts webui/src/api/__tests__/client.endpoints.test.ts
git commit -m "feat(webui): signal API client + types (remove finding)"
```

---

## Task 2: queries hooks

**Files:** `webui/src/lib/queries.ts`

- [ ] **Step 1: signals key + hook**
```typescript
signals: (id: string) => ['session', id, 'signals'] as const,

export function useSignalsQuery(id: string, opts?: QOpts<SignalDto[]>) {
  return useQuery<SignalDto[]>({
    queryKey: sessionKeys.signals(id),
    queryFn: async () => {
      const all = await getSignals(id);
      // Evidence-linked invariant: drop empty evidence_refs.
      return all.filter((s) => Array.isArray(s.evidence_refs) && s.evidence_refs.length > 0);
    },
    enabled: !!id,
    ...opts,
  });
}
```
Remove `findings`/`toolFailures`/`findingEvidence` keys, `useFindingsQuery`/`useToolFailureSummaryQuery`/`useFindingEvidenceQuery`, and their imports (`getFindings` etc.). Update the `getSignals` import.

- [ ] **Step 2: typecheck** — `cd webui && pnpm tsc -b` (or repo runner). Expect errors ONLY in consumers (SessionDetailPage etc.) handled in Task 3 — confirm `queries.ts` itself is clean. (If you prefer, defer the build to Task 3; just ensure no syntax error here.)

- [ ] **Step 3: Commit**
```bash
git add webui/src/lib/queries.ts
git commit -m "feat(webui): useSignalsQuery (remove finding hooks)"
```

---

## Task 3: SessionDetailPage + DetailPanel + InsightTab

**Files:** `webui/src/routes/SessionDetailPage.tsx`, `webui/src/components/replay/detail/DetailPanel.tsx`, `InsightTab.tsx`, `InsightTab.module.css`, `components/replay/detail/__tests__/InsightTab.test.tsx`, `routes/__tests__/SessionDetailPage.test.tsx`

- [ ] **Step 1: SessionDetailPage** — swap finding→signal:
  - `const signals = useSignalsQuery(sessionId);` (was `useFindingsQuery`)
  - `const signalsData = signals.data ?? [];`
  - `signalEventIds` (rename `findingEventIds`): same evidence_refs walk over `signalsData`.
  - `selectedNodeSignals` (rename `selectedNodeFindings`): filter `signalsData` by evidence_refs containing `selectedEventId`.
  - `<DetailPanel ... signals={selectedNodeSignals} />` (was `findings`).
  - `<ConversationStream ... findingEventIds={signalEventIds} />` — keep the prop name `findingEventIds` on ConversationStream/MessageCard for now (it's the "has insight marker" set; renaming is optional churn — leave it to minimize diff, OR rename to `signalEventIds` consistently if quick). Pick one and be consistent.
  - `<InsightStrip ... signals={signals.data} />` — remove `findings`/`toolFailures` props (Task 4 updates InsightStrip).
  - Remove the `toolFailures` query usage.

- [ ] **Step 2: DetailPanel.tsx** — `signals: SignalDto[]` prop (was `findings: FindingDto[]`), pass `signals={signals}` to InsightTab. Update import.

- [ ] **Step 3: InsightTab.tsx** — `SignalsList` replacing `FindingsList`:
```typescript
function SignalsList({ signals }: { signals: SignalDto[] }) {
  return (
    <ul className={styles.list}>
      {signals.map((s) => (
        <li key={s.signal_id} className={styles.item}>
          <div className={styles.head}>
            <span className={styles.detector}>{s.detector}</span>
            {s.subkind && <span className={styles.subkind}>{s.subkind}</span>}
          </div>
          <p className={styles.summary}>{s.summary}</p>
        </li>
      ))}
    </ul>
  );
}
```
Props `findings: FindingDto[]` → `signals: SignalDto[]`; the empty-state guard `(!event && signals.length === 0)`; section title "Signals"; render `<SignalsList signals={signals} />`. Remove `SEV_CLASS`/severity usage.

- [ ] **Step 4: InsightTab.module.css** — add `.detector`/`.subkind` (reuse `.category` styling for `.detector`; `.subkind` a muted chip). Remove now-unused `.sev*` if nothing else uses them.

- [ ] **Step 5: tests** — InsightTab.test.tsx: replace `finding()` fixture with `signal()` (no severity/confidence); assert detector + summary visible, and that severity/confidence text is absent. SessionDetailPage.test.tsx: change the fetch mock `/findings`→`/signals` (and drop `/tool-failures` mock); keep the existing render assertions.

- [ ] **Step 6: build + vitest** — `cd webui && pnpm tsc -b && pnpm vitest run`. Expected: typecheck clean, all tests pass.

- [ ] **Step 7: Commit**
```bash
git add webui/src/routes/SessionDetailPage.tsx webui/src/components/replay/detail/ webui/src/routes/__tests__/SessionDetailPage.test.tsx
git commit -m "feat(webui): detail view renders signals (SignalsList, signalEventIds)"
```

---

## Task 4: InsightStrip tool_failure card (signal-based minimal)

**Files:** `webui/src/components/replay/insight-strip/InsightStrip.tsx`, `insightCards.ts`, insight-strip card test

> KPI 전면 개편은 별도 트랙(메모리: defer KPI). 여기선 깨지지 않게 + tool_failure 카드를 signal 기준으로 최소 복구.

- [ ] **Step 1: InsightStrip.tsx** — prop `findings?: FindingDto[]` → `signals?: SignalDto[]`; remove `toolFailures` prop. Pass `signals` into `buildInsightCards`.

- [ ] **Step 2: insightCards.ts** — `InsightInputs.findings`→`signals: SignalDto[] | undefined`; remove `toolFailures`. Rewrite `toolFailureCard`:
```typescript
function toolFailureCard(inputs: InsightInputs): InsightCardModel {
  const tip = '도구 실패 signal 수(detector=tool_failure). 결정적 카운트이며 심각도 판단은 포함하지 않습니다.';
  const sigs = inputs.signals;
  if (!sigs) {
    return { id: 'tool_failure', title: '도구 실패', value: '—', detail: '로딩 중', provenance: 'uncollected', tooltip: tip };
  }
  const failures = sigs.filter((s) => s.detector === 'tool_failure');
  return {
    id: 'tool_failure', title: '도구 실패',
    value: `${failures.length}`,
    detail: failures.length === 0 ? '도구 실패 없음' : '펼쳐서 확인',
    provenance: 'measured', tooltip: tip,
    drill: { lines: failures.map((s) => `${s.subkind ?? s.detector} · ${s.summary}`) },
  };
}
```
Remove `TOOL_FAILURE_CATEGORIES` if now unused. Other cards (context/tokens/verification/cost) unchanged.

- [ ] **Step 3: tests** — update the insight-strip card test(s) that fed `findings`/`toolFailures` to feed `signals` with a `detector:'tool_failure'` entry; assert the count.

- [ ] **Step 4: build + vitest** — `cd webui && pnpm tsc -b && pnpm vitest run`. Expected PASS.

- [ ] **Step 5: Commit**
```bash
git add webui/src/components/replay/insight-strip/
git commit -m "feat(webui): InsightStrip tool_failure card from signals (deterministic count)"
```

---

## Task 5: full build + browser smoke

**Files:** none (verification)

- [ ] **Step 1: full vitest + typecheck** — `cd webui && pnpm tsc -b && pnpm vitest run`. Expected: 0 failures. Grep for stray finding refs: `grep -rn "FindingDto\|getFindings\|useFindingsQuery\|/findings\|tool-failures\|FindingEvidence" webui/src` → only comments allowed.

- [ ] **Step 2: production build** — `cd webui && pnpm build`. Expected: dist generated, no TS errors.

- [ ] **Step 3: rebuild embedded dist + browser smoke (controller does this)**
  This step is performed by the CONTROLLER (not the subagent), per project rule "UI는 브라우저 smoke 후 commit":
  - `cargo build` to embed the new dist, run `wimcc serve` on an INACTIVE/static session (memory: smoke on static session), navigate via claude-in-chrome, select a tool_call event, confirm the Insight tab shows signal(s) (detector/summary, no severity), and the message stream shows the insight marker. Visual check.

- [ ] **Step 4: Commit (if any smoke-driven fixes)** — otherwise Task 5 has no commit; the smoke is a gate before declaring Plan 2a done.

---

## Self-Review 메모
- 백엔드는 `tool-failures`/`finding evidence` 엔드포인트가 없으므로 그 프론트 코드는 **삭제**(전환 아님). user_visible/internal_retry 분류는 signal facts에 `retried` 등으로 남았으나 KPI 분류 UI는 deferred.
- `ConversationStream`/`MessageCard`의 `findingEventIds`/`hasFinding` prop명은 "insight marker"의 의미라 rename은 선택 — diff 최소화를 위해 유지하되 Task 3에서 일관성 결정.
- evidence_refs 구조(EvidenceRef: string | {event_id})는 finding과 signal이 동일 → marker 로직 재사용.
- 브라우저 smoke는 controller가 claude-in-chrome으로 (subagent는 vitest/build까지). static 세션 사용.
