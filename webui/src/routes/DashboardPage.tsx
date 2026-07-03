// B-1 프로젝트 대시보드 — 세션 횡단 트렌드 (2026-07-04 shadcn/Recharts 개편).
//
// 정보 구성·가로 시계열은 초판과 동일: 모든 차트가 같은 세션 축을 공유하고
// (Recharts syncId — hover·Brush 구간이 전 차트 동기), 모델 코호트는
// ReferenceArea 밴드 + 경계 ReferenceLine으로 표시한다. 코호트 원칙(경계는
// 관측된 비어있지 않은 값이 달라질 때만)의 SSOT는 lib/seriesView.ts.
// 판단 문장은 렌더하지 않는다(측정/판별 분리) — count·구간·경계만.
import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Bar,
  BarChart,
  Brush,
  CartesianGrid,
  ReferenceArea,
  ReferenceLine,
  XAxis,
  YAxis,
} from 'recharts';
import { getMetricsSeries, listSessions, ApiError } from '../api/client';
import type { MetricsSeriesDto, SessionListItem } from '../api/types';
import {
  sortSeriesAscending,
  cohortSegments,
  cohortBoundaries,
  cohortModels,
} from '../lib/seriesView';
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from '@/components/ui/chart';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { useT } from '../i18n';

type SeriesState =
  | { kind: 'loading' }
  | { kind: 'ok'; data: MetricsSeriesDto }
  | { kind: 'error'; message: string };

type WindowKey = '30d' | '90d' | 'all';
const WINDOW_DAYS: Record<Exclude<WindowKey, 'all'>, number> = { '30d': 30, '90d': 90 };

/* 코호트 밴드 — dataviz validator 통과 categorical 세트(빈도순 슬롯,
 * 초과분은 중립 슬레이트 "Other" 접기). */
const MODEL_SLOTS = ['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)'];
const MODEL_OVERFLOW = '#48536b';

const STRIP_METRICS = [
  'tool_failure_count',
  'context_bloat_count',
  'api_error_count',
  'user_interruption_count',
  'compact_boundary_count',
  'tool_result_truncated_count',
] as const;
type StripMetric = (typeof STRIP_METRICS)[number];

function basename(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
const shortId = (id: string) => id.slice(0, 8);
const fmtDate = (iso: string) => iso.slice(5, 10);

export default function DashboardPage() {
  const t = useT();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const [series, setSeries] = useState<SeriesState>({ kind: 'loading' });
  const [sessions, setSessions] = useState<SessionListItem[]>([]);

  const windowKey = (params.get('w') as WindowKey | null) ?? 'all';
  const projectParam = params.get('project');

  useEffect(() => {
    let alive = true;
    listSessions()
      .then((rows) => {
        if (alive) setSessions(rows);
      })
      .catch(() => {
        /* 선택지는 부가 기능 — series 오류가 본 오류를 보여준다 */
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

  useEffect(() => {
    if (projectParam === null && sessions.length === 0) return;
    let alive = true;
    setSeries({ kind: 'loading' });
    const from =
      windowKey === 'all'
        ? undefined
        : new Date(Date.now() - WINDOW_DAYS[windowKey] * 86_400_000).toISOString();
    getMetricsSeries({ project: effectiveProject, from, limit: 200 })
      .then((data) => {
        if (alive) setSeries({ kind: 'ok', data });
      })
      .catch((e) => {
        if (alive)
          setSeries({ kind: 'error', message: e instanceof ApiError ? e.detail : String(e) });
      });
    return () => {
      alive = false;
    };
  }, [effectiveProject, windowKey, projectParam, sessions.length]);

  const rows = useMemo(
    () => (series.kind === 'ok' ? sortSeriesAscending(series.data.sessions) : []),
    [series],
  );

  /* Recharts 데이터 — 세션 = 카테고리 축의 한 칸(등폭, 초판과 동일 의미). */
  const chartData = useMemo(
    () =>
      rows.map((r, i) => ({
        idx: i,
        sid: r.session_id,
        sid8: shortId(r.session_id),
        date: fmtDate(r.first_observed_at),
        passed: r.metrics.verification_passed,
        failed: r.metrics.verification_failed,
        unknown: r.metrics.verification_unknown,
        tool_failure_count: r.metrics.tool_failure_count,
        context_bloat_count: r.metrics.context_bloat_count,
        api_error_count: r.metrics.api_error_count,
        user_interruption_count: r.metrics.user_interruption_count,
        compact_boundary_count: r.metrics.compact_boundary_count,
        tool_result_truncated_count: r.metrics.tool_result_truncated_count,
        events: r.event_count,
        models: cohortModels(r.fingerprint).join(' + ').replaceAll('claude-', '') || '—',
        cc: r.fingerprint.cc_versions.join(' + ') || '—',
      })),
    [rows],
  );

  const modelSegments = useMemo(
    () => cohortSegments(rows, (r) => cohortModels(r.fingerprint)),
    [rows],
  );
  const boundaries = useMemo(() => cohortBoundaries(modelSegments), [modelSegments]);
  const slotOf = useMemo(() => {
    const weight = new Map<string, number>();
    for (const s of modelSegments) {
      if (!s.known) continue;
      weight.set(s.label, (weight.get(s.label) ?? 0) + (s.end - s.start + 1));
    }
    const ranked = [...weight.entries()].sort((a, b) =>
      b[1] !== a[1] ? b[1] - a[1] : a[0].localeCompare(b[0]),
    );
    const m = new Map<string, string>();
    ranked.forEach(([label], i) => m.set(label, i < MODEL_SLOTS.length ? MODEL_SLOTS[i] : MODEL_OVERFLOW));
    return m;
  }, [modelSegments]);

  const outcomeConfig = {
    passed: { label: t('dash.outcome.passed'), color: 'var(--chart-5)' },
    failed: { label: t('dash.outcome.failed'), color: '#e66767' },
    unknown: { label: t('dash.outcome.unknown'), color: '#6a7180' },
  } satisfies ChartConfig;

  const setParam = (key: string, value: string | null) => {
    const next = new URLSearchParams(params);
    if (value === null) next.delete(key);
    else next.set(key, value);
    setParams(next, { replace: true });
  };

  /* 차트 클릭 → 세션 replay 딥링크. recharts v3 onClick 파라미터의 활성
   * 인덱스만 쓴다(activeTooltipIndex — 카테고리 축이라 곧 chartData 인덱스). */
  const openSession = (state: unknown) => {
    const raw = (state as { activeTooltipIndex?: number | string } | null)?.activeTooltipIndex;
    const idx = typeof raw === 'string' ? Number(raw) : raw;
    const sid = typeof idx === 'number' && Number.isFinite(idx) ? chartData[idx]?.sid : undefined;
    if (sid) navigate(`/sessions/${encodeURIComponent(sid)}`);
  };

  /* 툴팁 라벨: 세션id · 날짜 · 모델 · CC — fingerprint를 hover에서 바로. */
  const tooltipLabel = (_: unknown, payload: readonly { payload?: (typeof chartData)[number] }[]) => {
    const p = payload?.[0]?.payload;
    if (!p) return null;
    return (
      <div className="space-y-0.5">
        <div className="font-mono">{p.sid8} · {p.date}</div>
        <div className="text-muted-foreground font-normal">
          {p.models} · CC {p.cc} · {t('dash.tooltip.events', p.events)}
        </div>
      </div>
    );
  };

  const maxOf = (metric: StripMetric) => Math.max(0, ...chartData.map((d) => d[metric]));
  const outcomeMax = Math.max(0, ...chartData.map((d) => d.passed + d.failed + d.unknown));

  const cohortRefs = (withLabel: boolean) => (
    <>
      {withLabel &&
        modelSegments
          .filter((s) => s.known)
          .map((s) => (
            <ReferenceArea
              key={`band-${s.start}`}
              x1={s.start}
              x2={s.end}
              fill={slotOf.get(s.label) ?? MODEL_OVERFLOW}
              fillOpacity={0.07}
            />
          ))}
      {boundaries.map((b) => (
        <ReferenceLine
          key={`rule-${b.index}`}
          x={b.index}
          stroke="var(--wimcc-warning)"
          strokeOpacity={0.5}
          strokeDasharray="4 3"
        />
      ))}
    </>
  );

  return (
    <div className="mx-auto max-w-6xl space-y-5 px-7 py-6">
      <header className="space-y-3">
        <div>
          <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
            {t('dash.eyebrow')}
          </p>
          <h1 className="font-mono text-2xl font-semibold tracking-tight">
            {effectiveProject ? basename(effectiveProject) : t('dash.allProjects')}
          </h1>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Select
            value={projectParam ?? ''}
            onValueChange={(v) => setParam('project', v === '__auto__' ? null : v)}
          >
            <SelectTrigger size="sm" className="w-56 font-mono text-xs" aria-label={t('dash.projectLabel')}>
              <SelectValue placeholder={projects[0] ? `${basename(projects[0])} (auto)` : t('dash.allProjects')} />
            </SelectTrigger>
            <SelectContent>
              {projects[0] && (
                <SelectItem value="__auto__" className="font-mono text-xs">
                  {basename(projects[0])} (auto)
                </SelectItem>
              )}
              <SelectItem value="all" className="font-mono text-xs">
                {t('dash.allProjects')}
              </SelectItem>
              {projects.map((p) => (
                <SelectItem key={p} value={p} className="font-mono text-xs">
                  {basename(p)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={windowKey}
            onValueChange={(v) => v && setParam('w', v === 'all' ? null : v)}
            aria-label={t('dash.windowLabel')}
          >
            <ToggleGroupItem value="30d" className="px-3 text-xs">{t('dash.window.30d')}</ToggleGroupItem>
            <ToggleGroupItem value="90d" className="px-3 text-xs">{t('dash.window.90d')}</ToggleGroupItem>
            <ToggleGroupItem value="all" className="px-3 text-xs">{t('dash.window.all')}</ToggleGroupItem>
          </ToggleGroup>
          {series.kind === 'ok' && (
            <span className="ml-auto font-mono text-xs text-muted-foreground">
              {t('dash.sessionCount', series.data.session_count)}
              {series.data.matched_count > series.data.session_count && (
                <span className="text-(--wimcc-warning)">
                  {' · '}
                  {t('dash.truncated', {
                    n: series.data.session_count,
                    m: series.data.matched_count,
                  })}
                </span>
              )}
            </span>
          )}
        </div>
      </header>

      {series.kind === 'loading' && (
        <div className="space-y-5">
          <Skeleton className="h-72 w-full rounded-xl" />
          <Skeleton className="h-96 w-full rounded-xl" />
        </div>
      )}
      {series.kind === 'error' && (
        <Card role="alert" className="border-destructive/40">
          <CardContent className="py-6 text-sm text-destructive">
            {t('dash.error')} — {series.message}
          </CardContent>
        </Card>
      )}
      {series.kind === 'ok' && rows.length === 0 && (
        <Card>
          <CardContent className="space-y-2 py-10 text-center text-sm text-muted-foreground">
            <p>{t('dash.empty')}</p>
            <p>
              <code className="rounded bg-muted px-2 py-1 font-mono text-xs">
                {t('dash.emptyHint')}
              </code>
            </p>
          </CardContent>
        </Card>
      )}

      {series.kind === 'ok' && rows.length > 0 && (
        <Tabs defaultValue="charts">
          <TabsList>
            <TabsTrigger value="charts">{t('dash.tab.charts')}</TabsTrigger>
            <TabsTrigger value="table">{t('dash.table.summary')}</TabsTrigger>
          </TabsList>

          <TabsContent value="charts" className="space-y-5">
            <Card>
              <CardHeader>
                <div className="flex items-baseline justify-between gap-3">
                  <CardTitle>{t('dash.outcome.title')}</CardTitle>
                  <Badge variant="outline" className="font-mono text-[10px] text-muted-foreground">
                    {outcomeMax > 0 ? t('dash.axis.max', outcomeMax) : t('dash.outcome.none')}
                  </Badge>
                </div>
                <CardDescription>{t('dash.outcome.tip')}</CardDescription>
              </CardHeader>
              <CardContent>
                <ChartContainer config={outcomeConfig} className="h-64 w-full">
                  <BarChart
                    data={chartData}
                    syncId="dash"
                    onClick={openSession}
                    margin={{ top: 18, right: 8, left: 8, bottom: 0 }}
                    className="cursor-pointer"
                  >
                    <CartesianGrid vertical={false} stroke="var(--border)" />
                    <XAxis
                      dataKey="idx"
                      tickFormatter={(v: number) => chartData[v]?.date ?? ''}
                      tickLine={false}
                      axisLine={false}
                      tickMargin={8}
                      minTickGap={40}
                      fontSize={10}
                    />
                    <YAxis width={28} tickLine={false} axisLine={false} fontSize={10} allowDecimals={false} />
                    {cohortRefs(true)}
                    <ChartTooltip content={<ChartTooltipContent labelFormatter={tooltipLabel} />} />
                    <ChartLegend content={<ChartLegendContent />} />
                    <Bar dataKey="passed" stackId="v" fill="var(--color-passed)" />
                    <Bar dataKey="failed" stackId="v" fill="var(--color-failed)" />
                    <Bar dataKey="unknown" stackId="v" fill="var(--color-unknown)" radius={[3, 3, 0, 0]} />
                    <Brush
                      dataKey="idx"
                      height={22}
                      travellerWidth={8}
                      stroke="var(--wimcc-border-strong)"
                      fill="var(--card)"
                      tickFormatter={(v: number) => chartData[v]?.date ?? ''}
                    />
                  </BarChart>
                </ChartContainer>
                {/* 코호트 칩 범례 — 밴드 인라인 라벨은 좁은 구간에서 겹쳐
                    칩으로 옮겼다(색=밴드 tint, 첫 등장 순). */}
                <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1">
                  <span className="text-[10px] uppercase tracking-wider text-muted-foreground/70">
                    {t('dash.cohort.models')}
                  </span>
                  {[...new Map(modelSegments.filter((s) => s.known).map((s) => [s.label, s])).values()].map(
                    (s) => (
                      <span
                        key={s.label}
                        className="inline-flex items-center gap-1.5 font-mono text-[11px] text-muted-foreground"
                      >
                        <span
                          className="size-2.5 rounded-[3px]"
                          style={{ background: slotOf.get(s.label) ?? MODEL_OVERFLOW }}
                        />
                        {s.label.replaceAll('claude-', '')}
                      </span>
                    ),
                  )}
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>{t('dash.multiples.title')}</CardTitle>
                <CardDescription>{t('dash.multiples.tip')}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                {STRIP_METRICS.map((metric) => (
                  <div key={metric}>
                    <div className="mb-1 flex items-baseline justify-between">
                      <span className="text-xs text-muted-foreground">
                        {t(`dash.metric.${metric}`)}
                      </span>
                      <span className="font-mono text-[10px] text-muted-foreground/70">
                        {t('dash.axis.max', maxOf(metric))}
                      </span>
                    </div>
                    <ChartContainer
                      config={{ [metric]: { label: t(`dash.metric.${metric}`), color: 'var(--wimcc-info)' } }}
                      className="h-16 w-full"
                    >
                      <BarChart
                        data={chartData}
                        syncId="dash"
                        onClick={openSession}
                        margin={{ top: 2, right: 8, left: 8, bottom: 0 }}
                        className="cursor-pointer"
                      >
                        <XAxis dataKey="idx" hide />
                        {/* 메인 차트와 플롯 영역 정렬 — 같은 폭의 숨은 YAxis. */}
                        <YAxis hide width={28} />
                        {cohortRefs(false)}
                        <ChartTooltip
                          content={<ChartTooltipContent labelFormatter={tooltipLabel} />}
                        />
                        <Bar dataKey={metric} fill="var(--wimcc-info)" radius={[2, 2, 0, 0]} />
                      </BarChart>
                    </ChartContainer>
                  </div>
                ))}
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="table">
            <Card>
              <CardContent className="overflow-x-auto pt-6">
                <Table className="font-mono text-xs">
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t('dash.table.session')}</TableHead>
                      <TableHead>{t('dash.table.date')}</TableHead>
                      <TableHead className="text-right">{t('dash.table.events')}</TableHead>
                      <TableHead className="text-right">{t('dash.outcome.passed')}</TableHead>
                      <TableHead className="text-right">{t('dash.outcome.failed')}</TableHead>
                      <TableHead className="text-right">{t('dash.outcome.unknown')}</TableHead>
                      {STRIP_METRICS.map((m) => (
                        <TableHead key={m} className="text-right">
                          {t(`dash.metric.${m}`)}
                        </TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {chartData.map((d) => (
                      <TableRow
                        key={d.sid}
                        className="cursor-pointer"
                        onClick={() => navigate(`/sessions/${encodeURIComponent(d.sid)}`)}
                      >
                        <TableCell>{d.sid8}</TableCell>
                        <TableCell>{d.date}</TableCell>
                        <TableCell className="text-right">{d.events}</TableCell>
                        <TableCell className="text-right">{d.passed}</TableCell>
                        <TableCell className="text-right">{d.failed}</TableCell>
                        <TableCell className="text-right">{d.unknown}</TableCell>
                        {STRIP_METRICS.map((m) => (
                          <TableCell key={m} className="text-right">
                            {d[m]}
                          </TableCell>
                        ))}
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
