// B-1 프로젝트 대시보드 — 세션 횡단 트렌드. "개선됐는가"라는 질문에 사람이
// 회고 LLM과 같은 데이터(/v1/metrics series + fingerprint)를 보게 한다.
//
// 구조: 모든 차트 블록(코호트 레일·outcome·metric strip)이 같은 세션 축
// (등폭 트랙, columns.ts)을 공유하고, 코호트 경계 룰은 단일 오버레이가
// 전 블록을 관통해 그린다 — 환경 변화와 지표 이동을 한 시선으로 잇는 것이
// 이 페이지의 존재 이유다. 판단은 렌더하지 않는다(측정/판별 분리): 여기엔
// count·구간·경계만 있고 "좋아졌다"는 문장은 없다.
import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { getMetricsSeries, listSessions, ApiError } from '../api/client';
import type { MetricsSeriesDto, SessionListItem, SessionMetricsDto } from '../api/types';
import {
  sortSeriesAscending,
  cohortSegments,
  cohortBoundaries,
  cohortModels,
} from '../lib/seriesView';
import { CohortRail } from '../components/dashboard/CohortRail';
import { ColumnsChart } from '../components/dashboard/ColumnsChart';
import { trackLeft } from '../components/dashboard/columns';
import { useT } from '../i18n';
import styles from './DashboardPage.module.css';

type SeriesState =
  | { kind: 'loading' }
  | { kind: 'ok'; data: MetricsSeriesDto }
  | { kind: 'error'; message: string };

type WindowKey = '30d' | '90d' | 'all';
const WINDOW_DAYS: Record<Exclude<WindowKey, 'all'>, number> = { '30d': 30, '90d': 90 };

/* 검증 outcome 상태색 — dataviz validator 통과쌍(green/red) + 의도된
 * de-emphasis gray(unknown = 신호 부재). 세그먼트 2px 갭 + 범례가 2차 부호. */
const OUTCOME_COLORS = { passed: '#199e70', failed: '#e66767', unknown: '#6a7180' } as const;
/* 프로세스 strip 단일 hue — 정체성은 strip 제목이 전달한다. */
const STRIP_COLOR = 'var(--wimcc-info)';

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

function shortId(id: string): string {
  return id.slice(0, 8);
}

function fmtDate(iso: string): string {
  return iso.slice(0, 10);
}

export default function DashboardPage() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const [series, setSeries] = useState<SeriesState>({ kind: 'loading' });
  const [sessions, setSessions] = useState<SessionListItem[]>([]);

  const windowKey = (params.get('w') as WindowKey | null) ?? 'all';
  const projectParam = params.get('project');

  // 프로젝트 선택지 — 세션 목록에서 파생(최근 활동순). 별도 API 없음.
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

  // 기본값: 가장 최근 활동 프로젝트. 'all'은 명시 선택.
  const effectiveProject =
    projectParam === 'all' ? undefined : (projectParam ?? projects[0] ?? undefined);

  useEffect(() => {
    // 프로젝트 선택지가 아직 안 왔고 기본값 대기 중이면 첫 로드는 미룬다.
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
  const modelSegments = useMemo(
    () => cohortSegments(rows, (r) => cohortModels(r.fingerprint)),
    [rows],
  );
  const ccSegments = useMemo(() => cohortSegments(rows, (r) => r.fingerprint.cc_versions), [rows]);
  // 관통 룰은 모델 집합 변화만 — CC 버전은 거의 매 세션 바뀌어(릴리스 주기)
  // 전부 그리면 소음이 된다. CC 경계는 레일 세그먼트 가장자리가 전달한다.
  const boundaryIndices = useMemo(
    () => cohortBoundaries(modelSegments).map((b) => b.index),
    [modelSegments],
  );

  const outcomeStacks = useMemo(
    () => [
      {
        key: 'passed',
        color: OUTCOME_COLORS.passed,
        values: rows.map((r) => r.metrics.verification_passed),
      },
      {
        key: 'failed',
        color: OUTCOME_COLORS.failed,
        values: rows.map((r) => r.metrics.verification_failed),
      },
      {
        key: 'unknown',
        color: OUTCOME_COLORS.unknown,
        values: rows.map((r) => r.metrics.verification_unknown),
      },
    ],
    [rows],
  );

  const setParam = (key: string, value: string | null) => {
    const next = new URLSearchParams(params);
    if (value === null) next.delete(key);
    else next.set(key, value);
    setParams(next, { replace: true });
  };

  const outcomeTooltip = (i: number) => {
    const r = rows[i];
    if (!r) return null;
    const m = r.metrics;
    return [
      `${shortId(r.session_id)} · ${fmtDate(r.first_observed_at)}`,
      `${t('dash.outcome.passed')} ${m.verification_passed} · ${t('dash.outcome.failed')} ${m.verification_failed} · ${t('dash.outcome.unknown')} ${m.verification_unknown}`,
    ].join('\n');
  };

  const stripTooltip = (metric: StripMetric) => (i: number) => {
    const r = rows[i];
    if (!r) return null;
    return `${shortId(r.session_id)} · ${fmtDate(r.first_observed_at)}\n${t(`dash.metric.${metric}`)} ${r.metrics[metric]}`;
  };

  const maxOf = (metric: StripMetric) => Math.max(0, ...rows.map((r) => r.metrics[metric]));
  const outcomeMax = Math.max(
    0,
    ...rows.map(
      (r) =>
        r.metrics.verification_passed + r.metrics.verification_failed + r.metrics.verification_unknown,
    ),
  );

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <p className={styles.eyebrow}>{t('dash.eyebrow')}</p>
        <h1 className={styles.title}>
          {effectiveProject ? basename(effectiveProject) : t('dash.allProjects')}
        </h1>
        <div className={styles.filters}>
          <label className={styles.filterLabel}>
            {t('dash.projectLabel')}
            <select
              className={styles.select}
              value={projectParam ?? (projects[0] ? '' : 'all')}
              onChange={(e) => setParam('project', e.target.value === '' ? null : e.target.value)}
            >
              {projects[0] && <option value="">{basename(projects[0])} (auto)</option>}
              <option value="all">{t('dash.allProjects')}</option>
              {projects.map((p) => (
                <option key={p} value={p}>
                  {basename(p)}
                </option>
              ))}
            </select>
          </label>
          <div className={styles.windowGroup} role="group" aria-label={t('dash.windowLabel')}>
            {(['30d', '90d', 'all'] as const).map((w) => (
              <button
                key={w}
                type="button"
                className={windowKey === w ? styles.windowOn : styles.windowBtn}
                aria-pressed={windowKey === w}
                onClick={() => setParam('w', w === 'all' ? null : w)}
              >
                {t(`dash.window.${w}`)}
              </button>
            ))}
          </div>
        </div>
        {series.kind === 'ok' && (
          <p className={styles.meta}>
            {t('dash.sessionCount', series.data.session_count)}
            {series.data.matched_count > series.data.session_count && (
              <span className={styles.truncNote}>
                {' · '}
                {t('dash.truncated', { n: series.data.session_count, m: series.data.matched_count })}
              </span>
            )}
          </p>
        )}
      </header>

      {series.kind === 'loading' && <p className={styles.state}>{t('dash.loading')}</p>}
      {series.kind === 'error' && (
        <p className={styles.stateError} role="alert">
          {t('dash.error')} — {series.message}
        </p>
      )}
      {series.kind === 'ok' && rows.length === 0 && (
        <div className={styles.state}>
          <p>{t('dash.empty')}</p>
          <p className={styles.hint}>
            <code>{t('dash.emptyHint')}</code>
          </p>
        </div>
      )}

      {series.kind === 'ok' && rows.length > 0 && (
        <>
          <div className={styles.charts}>
            {/* 경계 오버레이 — 모든 블록 관통. */}
            <div className={styles.overlay} aria-hidden="true">
              {boundaryIndices.map((idx) => (
                <span key={idx} className={styles.rule} style={{ left: trackLeft(idx, rows.length) }} />
              ))}
            </div>

            <section className={styles.block} aria-label={t('dash.cohort.title')}>
              <div className={styles.blockHead}>
                <h2 className={styles.blockTitle}>{t('dash.cohort.title')}</h2>
                <span className={styles.blockTip}>{t('dash.cohort.tip')}</span>
              </div>
              <CohortRail
                bandLabel={t('dash.cohort.models')}
                segments={modelSegments}
                total={rows.length}
                kind="categorical"
                unknownLabel={t('dash.cohort.unknown')}
              />
              <CohortRail
                bandLabel={t('dash.cohort.cc')}
                segments={ccSegments}
                total={rows.length}
                kind="ordinal"
                unknownLabel={t('dash.cohort.unknown')}
              />
            </section>

            <section className={styles.block} aria-label={t('dash.outcome.title')}>
              <div className={styles.blockHead}>
                <h2 className={styles.blockTitle}>{t('dash.outcome.title')}</h2>
                <span className={styles.maxTag}>
                  {outcomeMax > 0 ? t('dash.axis.max', outcomeMax) : t('dash.outcome.none')}
                </span>
                <span className={styles.blockTip}>{t('dash.outcome.tip')}</span>
              </div>
              <ColumnsChart
                rows={rows}
                stacks={outcomeStacks}
                height={140}
                ariaLabel={t('dash.outcome.title')}
                tooltip={outcomeTooltip}
                openSessionLabel={(id) => t('dash.openSession', shortId(id))}
              />
              <div className={styles.legend}>
                {(['passed', 'failed', 'unknown'] as const).map((k) => (
                  <span key={k} className={styles.legendItem}>
                    <span className={styles.swatch} style={{ background: OUTCOME_COLORS[k] }} />
                    {t(`dash.outcome.${k}`)}
                  </span>
                ))}
              </div>
            </section>

            <section className={styles.block} aria-label={t('dash.multiples.title')}>
              <div className={styles.blockHead}>
                <h2 className={styles.blockTitle}>{t('dash.multiples.title')}</h2>
                <span className={styles.blockTip}>{t('dash.multiples.tip')}</span>
              </div>
              <div className={styles.strips}>
                {STRIP_METRICS.map((metric) => (
                  <div key={metric} className={styles.strip}>
                    <div className={styles.stripHead}>
                      <span className={styles.stripTitle}>{t(`dash.metric.${metric}`)}</span>
                      <span className={styles.maxTag}>{t('dash.axis.max', maxOf(metric))}</span>
                    </div>
                    <ColumnsChart
                      rows={rows}
                      stacks={[
                        { key: metric, color: STRIP_COLOR, values: rows.map((r) => r.metrics[metric]) },
                      ]}
                      height={44}
                      ariaLabel={t(`dash.metric.${metric}`)}
                      tooltip={stripTooltip(metric)}
                      openSessionLabel={(id) => t('dash.openSession', shortId(id))}
                    />
                  </div>
                ))}
              </div>
            </section>
          </div>

          <div className={styles.axis} aria-hidden="true">
            <span>{fmtDate(rows[0].first_observed_at)}</span>
            <span>{fmtDate(rows[rows.length - 1].first_observed_at)}</span>
          </div>

          <details className={styles.tableBox}>
            <summary>{t('dash.table.summary')}</summary>
            <div className={styles.tableScroll}>
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th>{t('dash.table.session')}</th>
                    <th>{t('dash.table.date')}</th>
                    <th>{t('dash.table.events')}</th>
                    <th>{t('dash.outcome.passed')}</th>
                    <th>{t('dash.outcome.failed')}</th>
                    <th>{t('dash.outcome.unknown')}</th>
                    {STRIP_METRICS.map((m) => (
                      <th key={m}>{t(`dash.metric.${m}`)}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <tr key={r.session_id}>
                      <td>
                        <code>{shortId(r.session_id)}</code>
                      </td>
                      <td>{fmtDate(r.first_observed_at)}</td>
                      <td>{r.event_count}</td>
                      <td>{r.metrics.verification_passed}</td>
                      <td>{r.metrics.verification_failed}</td>
                      <td>{r.metrics.verification_unknown}</td>
                      {STRIP_METRICS.map((m) => (
                        <td key={m}>{(r.metrics as SessionMetricsDto)[m]}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
        </>
      )}
    </div>
  );
}
