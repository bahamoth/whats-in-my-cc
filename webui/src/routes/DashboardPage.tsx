// 프로젝트 대시보드 — 2026-07-04 전면 개편 (스펙 docs/specs/2026-07-04-dashboard-redesign.md,
// 승인 목업 docs/mockups/dash-full-mockup.html · dash-verification-mockup.html).
//
// 위계: 결정론 지표의 문자 헤드라인(이전 동일 창 delta) → 하위 시각화.
// 개요 탭: 일별 검증 → 일별 비용·신호 → 코호트 비교 → 세션 타임라인 → 세션 분포.
// 검증 탭: /v1/verification/summary 집계(측정률·행방·리듬·커버리지).
// 판정 문장은 렌더하지 않는다 — 숫자·delta·관측 사실만(측정/판별 분리).
// 파생의 SSOT는 lib/dashDerive(vitest), 집계의 SSOT는 백엔드 insight::verification_summary.
import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { getMetricsSeries, getVerificationSummary, listSessions, ApiError } from '../api/client';
import type { MetricsSeriesDto, SessionListItem, VerificationSummaryDto } from '../api/types';
import { sortSeriesAscending } from '../lib/seriesView';
import {
  buildDaily,
  cohortCompare,
  headline,
  headlineDelta,
  observedChanges,
  signalsOf,
} from '../lib/dashDerive';
import { HeadlineStats } from '../components/dash/HeadlineStats';
import { DailyVerification } from '../components/dash/DailyVerification';
import { DailyCostSignals } from '../components/dash/DailyCostSignals';
import type { CohortMarker, DayDetail } from '../components/dash/dailyOptions';
import { CohortCompareCards } from '../components/dash/CohortCompare';
import { SessionCardLane } from '../components/dash/SessionCardLane';
import { SessionScatter } from '../components/dash/SessionScatter';
import { VerificationTab } from '../components/dash/VerificationTab';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Skeleton } from '@/components/ui/skeleton';
import { DateRangeControl, type WindowSel } from '../components/dash/DateRangeControl';
import { useT } from '../i18n';

type SeriesState =
  | { kind: 'loading' }
  | { kind: 'ok'; data: MetricsSeriesDto }
  | { kind: 'error'; message: string };

type VsumState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ok'; data: VerificationSummaryDto }
  | { kind: 'error' };

type WindowKey = '30d' | '90d' | 'all' | 'custom';
const WINDOW_DAYS: Record<'30d' | '90d', number> = { '30d': 30, '90d': 90 };

/** URL 파라미터 → 창 선택. custom은 from/to(YYYY-MM-DD)가 모두 있어야 성립. */
function readWindow(params: URLSearchParams): WindowSel {
  const w = params.get('w');
  if (w === 'custom') {
    const from = params.get('from');
    const to = params.get('to');
    if (from && to) return { kind: 'custom', from, to };
    return { kind: 'all' };
  }
  if (w === '30d' || w === '90d') return { kind: w };
  return { kind: 'all' };
}

/** 창 선택 → fetch 범위(ISO)와 직전 동일 창. all은 비교 없음. */
function windowRange(sel: WindowSel): {
  from?: string;
  to?: string;
  prevFrom?: string;
  prevTo?: string;
} {
  if (sel.kind === 'all') return {};
  if (sel.kind === 'custom') {
    const fromMs = Date.parse(`${sel.from}T00:00:00Z`);
    const toMs = Date.parse(`${sel.to}T23:59:59Z`);
    const span = toMs - fromMs;
    return {
      from: new Date(fromMs).toISOString(),
      to: new Date(toMs).toISOString(),
      prevFrom: new Date(fromMs - span).toISOString(),
      prevTo: new Date(fromMs).toISOString(),
    };
  }
  const span = WINDOW_DAYS[sel.kind] * 86_400_000;
  const now = Date.now();
  return {
    from: new Date(now - span).toISOString(),
    prevFrom: new Date(now - 2 * span).toISOString(),
    prevTo: new Date(now - span).toISOString(),
  };
}

function basename(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export default function DashboardPage() {
  const t = useT();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const [series, setSeries] = useState<SeriesState>({ kind: 'loading' });
  const [prevSeries, setPrevSeries] = useState<MetricsSeriesDto | null>(null);
  const [vsum, setVsum] = useState<VsumState>({ kind: 'idle' });
  const [sessions, setSessions] = useState<SessionListItem[]>([]);

  const windowSel = readWindow(params);
  const windowKey: WindowKey = windowSel.kind;
  const projectParam = params.get('project');
  const tab = params.get('tab') === 'verification' ? 'verification' : 'overview';

  useEffect(() => {
    let alive = true;
    listSessions()
      .then((rows) => {
        if (alive) setSessions(rows);
      })
      .catch(() => {
        /* 선택지·이름 해석은 부가 기능 — series 오류가 본 오류를 보여준다 */
      });
    return () => {
      alive = false;
    };
  }, []);

  const projects = useMemo(() => {
    const latest = new Map<string, string>();
    for (const s of sessions) {
      if (!s.project) continue;
      const cur = latest.get(s.project);
      if (!cur || s.last_observed_at > cur) latest.set(s.project, s.last_observed_at);
    }
    return [...latest.entries()].sort((a, b) => b[1].localeCompare(a[1])).map(([p]) => p);
  }, [sessions]);

  const effectiveProject =
    projectParam === 'all' ? undefined : (projectParam ?? projects[0] ?? undefined);

  /** 현재 창 + (창이 유한하면) 직전 동일 창 — 헤드라인 delta의 기준. */
  useEffect(() => {
    if (projectParam === null && sessions.length === 0) return;
    let alive = true;
    setSeries({ kind: 'loading' });
    setPrevSeries(null);
    setVsum({ kind: 'idle' });
    const range = windowRange(readWindow(params));
    getMetricsSeries({ project: effectiveProject, from: range.from, to: range.to, limit: 200 })
      .then((data) => {
        if (alive) setSeries({ kind: 'ok', data });
      })
      .catch((e) => {
        if (alive)
          setSeries({ kind: 'error', message: e instanceof ApiError ? e.detail : String(e) });
      });
    if (range.prevFrom) {
      getMetricsSeries({
        project: effectiveProject,
        from: range.prevFrom,
        to: range.prevTo,
        limit: 200,
      })
        .then((data) => {
          if (alive) setPrevSeries(data);
        })
        .catch(() => {
          /* 비교 창 실패 → delta 없음(fnote '비교 없음') — 본 데이터는 무관 */
        });
    }
    return () => {
      alive = false;
    };
    // params는 windowKey·from·to로 정규화되어 반영된다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [effectiveProject, windowKey, params.get('from'), params.get('to'), projectParam, sessions.length]);

  /** 검증 탭 lazy fetch — 같은 프로젝트·창 컨텍스트.
   *  주의: vsum.kind를 deps에 넣으면 setVsum(loading)이 effect를 재실행시켜
   *  cleanup이 자기 fetch를 취소한다(loading 고착 버그 — 페이지 테스트가
   *  잠금). 재진입 가드는 deps 밖에서 읽고, cleanup은 진행 중이던 loading을
   *  idle로 되돌려 탭 재진입 시 재요청되게 한다. */
  useEffect(() => {
    if (tab !== 'verification' || series.kind !== 'ok') return;
    if (vsum.kind !== 'idle') return;
    let cancelled = false;
    setVsum({ kind: 'loading' });
    const range = windowRange(readWindow(params));
    getVerificationSummary({
      project: effectiveProject,
      from: range.from,
      to: range.to,
    })
      .then((data) => {
        if (!cancelled) setVsum({ kind: 'ok', data });
      })
      .catch(() => {
        if (!cancelled) setVsum({ kind: 'error' });
      });
    return () => {
      cancelled = true;
      setVsum((p) => (p.kind === 'loading' ? { kind: 'idle' } : p));
    };
    // vsum.kind는 재진입 가드로만 읽는다 — deps에 넣으면 자기취소 루프.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, series.kind, effectiveProject, windowKey, params.get('from'), params.get('to')]);

  const rows = useMemo(
    () => (series.kind === 'ok' ? sortSeriesAscending(series.data.sessions) : []),
    [series],
  );
  const prevRows = useMemo(
    () => (prevSeries ? sortSeriesAscending(prevSeries.sessions) : null),
    [prevSeries],
  );

  const nameOf = useMemo(() => {
    const bySid = new Map(sessions.map((s) => [s.session_id, s.slug ?? s.session_id.slice(0, 8)]));
    return (sid: string) => bySid.get(sid) ?? sid.slice(0, 8);
  }, [sessions]);

  const h = useMemo(() => headline(rows), [rows]);
  const delta = useMemo(
    () => (prevRows && prevRows.length > 0 ? headlineDelta(h, headline(prevRows)) : null),
    [h, prevRows],
  );
  const changes = useMemo(() => observedChanges(rows), [rows]);
  const daily = useMemo(() => buildDaily(rows), [rows]);
  /** 코호트 전환 마커 — 관측된 변화 중 시간축에 놓을 수 있는 것(첫 관측·CC 전환). */
  const markers = useMemo<CohortMarker[]>(
    () =>
      changes.flatMap((c) => {
        if (c.kind === 'top_signals') return [];
        const date = c.kind === 'model_first' ? c.date : c.lastDate;
        const dayIdx = daily.dates.indexOf(date);
        if (dayIdx < 0) return [];
        return [
          {
            dayIdx,
            label: c.kind === 'model_first' ? t('dash.marker.first', c.model) : `Claude Code ${c.to}`,
          },
        ];
      }),
    [changes, daily, t],
  );
  const dayDetails = useMemo<DayDetail[][]>(
    () =>
      daily.sessionsOf.map((idxs) =>
        idxs.map((ri) => {
          const r = rows[ri];
          return {
            name: nameOf(r.session_id),
            cost: Math.round(r.metrics.estimated_cost_usd ?? 0),
            passed: r.metrics.verification_passed,
            guards: r.metrics.verification_total,
            signals: signalsOf(r.metrics),
          };
        }),
      ),
    [daily, rows, nameOf],
  );
  const cohort = useMemo(() => cohortCompare(rows), [rows]);
  const zeroGuards = useMemo(
    () => rows.filter((r) => r.metrics.verification_total === 0).length,
    [rows],
  );

  const setParam = (key: string, value: string | null) => {
    const next = new URLSearchParams(params);
    if (value === null) next.delete(key);
    else next.set(key, value);
    setParams(next, { replace: true });
  };

  return (
    <div className="px-8 py-6">
      {/* ── 상단: 타이틀 · 프로젝트 · 창 ─────────────────── */}
      <div className="mb-4 flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <p className="text-[10.5px] font-semibold tracking-[.09em] uppercase text-(--wimcc-fg-subtle)">
            {t('dash.eyebrow')}
          </p>
          <h1 className="text-lg font-semibold tracking-tight">
            {effectiveProject ? basename(effectiveProject) : t('dash.allProjects')}
            {series.kind === 'ok' && (
              <span className="ml-2 text-sm font-medium text-(--wimcc-fg-muted)">
                · {t('dash.sessionCount', rows.length)} · {h.events.toLocaleString()} events
              </span>
            )}
          </h1>
        </div>
        <div className="flex items-center gap-3">
          <Select
            value={projectParam ?? effectiveProject ?? 'all'}
            onValueChange={(v) => setParam('project', v)}
          >
            <SelectTrigger className="w-56" aria-label={t('dash.projectLabel')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('dash.allProjects')}</SelectItem>
              {projects.map((p) => (
                <SelectItem key={p} value={p}>
                  {basename(p)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <DateRangeControl
            sel={windowSel}
            onChange={(next) => {
              const p = new URLSearchParams(params);
              if (next.kind === 'custom') {
                p.set('w', 'custom');
                p.set('from', next.from);
                p.set('to', next.to);
              } else {
                p.set('w', next.kind);
                p.delete('from');
                p.delete('to');
              }
              setParams(p, { replace: true });
            }}
          />
        </div>
      </div>

      {series.kind === 'loading' && (
        <div className="space-y-4" aria-label={t('dash.loading')}>
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      )}

      {series.kind === 'error' && (
        <p role="alert" className="text-sm text-(--wimcc-danger)">
          {t('dash.error')} {series.message}
        </p>
      )}

      {series.kind === 'ok' && rows.length === 0 && (
        <div className="py-16 text-center text-sm text-(--wimcc-fg-muted)">
          <p>{t('dash.empty')}</p>
          <p className="mt-1 font-mono text-xs text-(--wimcc-fg-subtle)">{t('dash.emptyHint')}</p>
        </div>
      )}

      {series.kind === 'ok' && rows.length > 0 && (
        <Tabs value={tab} onValueChange={(v) => setParam('tab', v === 'overview' ? null : v)}>
          <TabsList>
            <TabsTrigger value="overview">{t('dash.tab.overview')}</TabsTrigger>
            <TabsTrigger value="verification">{t('dash.tab.verification')}</TabsTrigger>
          </TabsList>

          <TabsContent value="overview">
            {series.data.matched_count > series.data.session_count && (
              <p className="mb-2 text-xs text-(--wimcc-fg-subtle)">
                {t('dash.truncated', { n: series.data.session_count, m: series.data.matched_count })}
              </p>
            )}
            <HeadlineStats h={h} d={delta} />
            {changes.length > 0 && (
              <p className="mt-3 mb-1 text-[12.5px] text-(--wimcc-fg-muted)">
                <span className="mr-2 text-[10.5px] font-semibold tracking-[.09em] uppercase text-(--wimcc-fg-subtle)">
                  {t('dash.observed')}
                </span>
                {changes.map((c, i) => (
                  <span key={i} className="mr-3.5 font-mono text-[11.5px] text-[#b07dff]">
                    {c.kind === 'model_first' && t('dash.observed.modelFirst', c)}
                    {c.kind === 'cc_span' && t('dash.observed.ccSpan', c)}
                    {c.kind === 'top_signals' &&
                      t('dash.observed.topSignals', { name: nameOf(c.sessionId), n: c.n })}
                  </span>
                ))}
              </p>
            )}
            <DailyVerification
              daily={daily}
              markers={markers}
              details={dayDetails}
              zeroGuards={zeroGuards}
              guards={h.guards}
              passed={rows.reduce((a, r) => a + r.metrics.verification_passed, 0)}
            />
            <DailyCostSignals daily={daily} markers={markers} details={dayDetails} />
            <CohortCompareCards c={cohort} />
            <SessionCardLane
              rows={rows}
              nameOf={nameOf}
              onOpen={(sid) => navigate(`/sessions/${sid}`)}
              markers={markers}
            />
            <SessionScatter
              rows={rows}
              nameOf={nameOf}
              onOpen={(sid) => navigate(`/sessions/${sid}`)}
            />
          </TabsContent>

          <TabsContent value="verification">
            {vsum.kind === 'loading' && <Skeleton className="h-40 w-full" />}
            {vsum.kind === 'error' && (
              <p role="alert" className="text-sm text-(--wimcc-danger)">
                {t('dash.ver.error')}
              </p>
            )}
            {vsum.kind === 'ok' && <VerificationTab sum={vsum.data} nameOf={nameOf} />}
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
