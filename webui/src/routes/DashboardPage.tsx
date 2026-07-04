// 프로젝트 대시보드 — 2026-07-04 전면 개편 (스펙 docs/specs/2026-07-04-dashboard-redesign.md,
// 승인 목업 docs/mockups/dash-full-mockup.html · dash-verification-mockup.html).
//
// 위계: 결정론 지표의 문자 헤드라인(이전 동일 창 delta) → 하위 시각화.
// 개요 탭: 일별 검증 → 일별 비용·신호 → 코호트 비교 → 세션 타임라인 → 세션 분포.
// 검증 탭: /v1/verification/summary 집계(측정률·행방·리듬·커버리지).
// 판정 문장은 렌더하지 않는다 — 숫자·delta·관측 사실만(측정/판별 분리).
// 파생의 SSOT는 lib/dashDerive(vitest), 집계의 SSOT는 백엔드 insight::verification_summary.
import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { getMetricsSeries, getVerificationSummary, listSessions, ApiError } from '../api/client';
import type { MetricsSeriesDto, SessionListItem, VerificationSummaryDto } from '../api/types';
import { sortSeriesAscending } from '../lib/seriesView';
import { headline, headlineDelta, observedChanges } from '../lib/dashDerive';
import { HeadlineStats } from '../components/dash/HeadlineStats';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Skeleton } from '@/components/ui/skeleton';
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

type WindowKey = '30d' | '90d' | 'all';
const WINDOW_DAYS: Record<Exclude<WindowKey, 'all'>, number> = { '30d': 30, '90d': 90 };

function basename(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export default function DashboardPage() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const [series, setSeries] = useState<SeriesState>({ kind: 'loading' });
  const [prevSeries, setPrevSeries] = useState<MetricsSeriesDto | null>(null);
  const [vsum, setVsum] = useState<VsumState>({ kind: 'idle' });
  const [sessions, setSessions] = useState<SessionListItem[]>([]);

  const windowKey = (params.get('w') as WindowKey | null) ?? 'all';
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
    const spanMs = windowKey === 'all' ? null : WINDOW_DAYS[windowKey] * 86_400_000;
    const now = Date.now();
    const from = spanMs ? new Date(now - spanMs).toISOString() : undefined;
    getMetricsSeries({ project: effectiveProject, from, limit: 200 })
      .then((data) => {
        if (alive) setSeries({ kind: 'ok', data });
      })
      .catch((e) => {
        if (alive)
          setSeries({ kind: 'error', message: e instanceof ApiError ? e.detail : String(e) });
      });
    if (spanMs) {
      getMetricsSeries({
        project: effectiveProject,
        from: new Date(now - 2 * spanMs).toISOString(),
        to: new Date(now - spanMs).toISOString(),
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
  }, [effectiveProject, windowKey, projectParam, sessions.length]);

  /** 검증 탭 lazy fetch — 같은 프로젝트·창 컨텍스트. */
  useEffect(() => {
    if (tab !== 'verification' || vsum.kind !== 'idle' || series.kind !== 'ok') return;
    let alive = true;
    setVsum({ kind: 'loading' });
    const spanMs = windowKey === 'all' ? null : WINDOW_DAYS[windowKey] * 86_400_000;
    getVerificationSummary({
      project: effectiveProject,
      from: spanMs ? new Date(Date.now() - spanMs).toISOString() : undefined,
    })
      .then((data) => {
        if (alive) setVsum({ kind: 'ok', data });
      })
      .catch(() => {
        if (alive) setVsum({ kind: 'error' });
      });
    return () => {
      alive = false;
    };
  }, [tab, vsum.kind, series.kind, effectiveProject, windowKey]);

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
          <ToggleGroup
            type="single"
            value={windowKey}
            onValueChange={(v) => v && setParam('w', v)}
            aria-label={t('dash.windowLabel')}
          >
            <ToggleGroupItem value="30d">{t('dash.window.30d')}</ToggleGroupItem>
            <ToggleGroupItem value="90d">{t('dash.window.90d')}</ToggleGroupItem>
            <ToggleGroupItem value="all">{t('dash.window.all')}</ToggleGroupItem>
          </ToggleGroup>
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
                    {c.kind === 'cc_change' && t('dash.observed.ccChange', c)}
                    {c.kind === 'top_signals' &&
                      t('dash.observed.topSignals', { name: nameOf(c.sessionId), n: c.n })}
                  </span>
                ))}
              </p>
            )}
            {/* 모듈 2~6: 일별 검증 / 일별 비용·신호 / 코호트 비교 / 세션
                타임라인 / 세션 분포 — Task 5~8에서 장착 */}
          </TabsContent>

          <TabsContent value="verification">
            {vsum.kind === 'loading' && <Skeleton className="h-40 w-full" />}
            {vsum.kind === 'error' && (
              <p role="alert" className="text-sm text-(--wimcc-danger)">
                {t('dash.ver.error')}
              </p>
            )}
            {vsum.kind === 'ok' && (
              /* Task 9에서 VerificationTab 모듈로 대체 */
              <p className="font-mono text-sm text-(--wimcc-fg-muted)">
                {t('dash.head.guards', vsum.data.total)}
              </p>
            )}
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
