import { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { getMetricsSeries, listSessions, ApiError } from '../api/client';
import type { SessionListItem, SessionMetricsDto } from '../api/types';
import { signalsOf } from '../lib/dashDerive';
import { usageRatios } from '../lib/seriesView';
import { useLiveStream, type LiveEnvelope } from '../hooks/useLiveStream';
import { relativeTime, formatModel, formatSpan, spanMs } from '../lib/format';
import { groupTeamRows } from '../lib/teamGrouping';
import { agentColor } from '../lib/colorHash';
import { useLocale, useT } from '../i18n';
import styles from './SessionListPage.module.css';

type SortKey =
  | 'last_observed_at'
  | 'span'
  | 'event_count'
  | 'session_id'
  | 'verification'
  | 'signals'
  | 'cost'
  | 'rate'
  | 'hit';
type SortDir = 'asc' | 'desc';

const SORT_LABELS: Record<SortKey, string> = {
  session_id: 'session',
  last_observed_at: 'last seen',
  span: 'span',
  event_count: 'events',
  verification: 'verify',
  signals: 'signals',
  cost: 'cost',
  rate: '$/1M',
  hit: 'hit',
};

type State =
  | { kind: 'loading' }
  | { kind: 'ok'; rows: SessionListItem[] }
  | { kind: 'error'; message: string };

// slice-7 — collapse by_kind into a compact source-mix tag so users can spot
// transcript-only / OTel-only / hook-only sessions at a glance.
//
// Note on "OTel-only" rows: OTel `session.id` is the same value Claude Code
// uses as the transcript JSONL filename (i.e. the conversation UUID). A
// session that shows up with only OTel events is an *empty conversation* —
// the user started claude (SessionStart hook fires, OTel SDK initialises)
// but exited before any transcript line was written. Not a correlation
// failure. Verified against ~/.claude/session-env/<uuid>/ + docs at
// code.claude.com/docs/en/monitoring-usage (`session.id` = "Unique session
// identifier").
const TRANSCRIPT_KINDS = new Set([
  'user_message',
  'assistant_message',
  'thinking',
  'tool_call',
  'tool_result',
  'system_summary',
  'attachment_meta',
  'file_history_snapshot',
  'session_state',
]);
const OTEL_KINDS = new Set(['otel_span', 'metric_sample', 'log_record']);

type SourceMix = {
  transcript: number;
  otel: number;
  hook: number;
};

function sourceMix(byKind?: Record<string, number>): SourceMix {
  const mix: SourceMix = { transcript: 0, otel: 0, hook: 0 };
  if (!byKind) return mix;
  for (const [k, v] of Object.entries(byKind)) {
    if (TRANSCRIPT_KINDS.has(k)) mix.transcript += v;
    else if (OTEL_KINDS.has(k)) mix.otel += v;
    else if (k === 'hook_event') mix.hook += v;
  }
  return mix;
}

// S6 — project pill shows the last path segment of the cwd ("whats-in-my-cc"),
// not the full absolute path. The full path stays available on hover (title).
function projectBasename(p?: string): string | undefined {
  if (!p) return undefined;
  const parts = p.replace(/\/+$/, '').split('/');
  return parts[parts.length - 1] || p;
}

const LIVE_THRESHOLD_MS = 60_000;

function isLive(envelopeAtMs: number | undefined, nowMs: number): boolean {
  // slice-8 — flag sessions whose last SSE envelope arrived within the live
  // window. Honest signal: turns OFF when stream goes silent, ON when an
  // envelope arrives.
  if (envelopeAtMs === undefined) return false;
  return nowMs - envelopeAtMs <= LIVE_THRESHOLD_MS;
}

function compare(
  a: SessionListItem,
  b: SessionListItem,
  key: SortKey,
  metrics: Map<string, SessionMetricsDto>,
): number {
  const num = (r: SessionListItem): number => {
    const m = metrics.get(r.session_id);
    if (!m) return Number.NEGATIVE_INFINITY;
    const ratios = usageRatios(m);
    switch (key) {
      case 'verification': {
        const t = m.verification_passed + m.verification_failed;
        return t > 0 ? m.verification_passed / t : Number.NEGATIVE_INFINITY;
      }
      case 'signals':
        return signalsOf(m);
      case 'cost':
        return ratios.measured ? (m.estimated_cost_usd ?? 0) : Number.NEGATIVE_INFINITY;
      case 'rate':
        return ratios.measured ? ratios.unitRatePerM : Number.NEGATIVE_INFINITY;
      case 'hit':
        return ratios.measured ? ratios.cacheHitPct : Number.NEGATIVE_INFINITY;
      default:
        return 0;
    }
  };
  switch (key) {
    case 'event_count':
      return a.event_count - b.event_count;
    case 'session_id':
      return (a.slug ?? a.session_id).localeCompare(b.slug ?? b.session_id);
    case 'last_observed_at':
      return a.last_observed_at.localeCompare(b.last_observed_at);
    case 'span':
      return (
        (spanMs(a.first_observed_at, a.last_observed_at) ?? 0) -
        (spanMs(b.first_observed_at, b.last_observed_at) ?? 0)
      );
    default:
      return num(a) - num(b);
  }
}

function SortIndicator({ active, dir }: { active: boolean; dir: SortDir }) {
  if (!active) return <span className={styles.sortInactive}> ·</span>;
  return <span className={styles.sortActive}> {dir === 'desc' ? '▼' : '▲'}</span>;
}

// S6 — case-insensitive substring match across the identity facets so the
// search box finds a session by slug, project, model, UUID, or preview text.
function matchesQuery(r: SessionListItem, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return [r.slug, r.project, r.session_id, r.first_user_message_preview, r.model, r.agent_name, r.team_name]
    .some((f) => f?.toLowerCase().includes(needle));
}

/** 지표 5셀 — 미측정은 '—'(0 위장 금지). 검증은 통과/전체 + pass/fail 마이크로바. */
function MetricCells({ m }: { m?: SessionMetricsDto }) {
  const dim = <td className={styles.eventsCell}><span className={styles.tagDim}>—</span></td>;
  if (!m) return <>{dim}{dim}{dim}{dim}{dim}</>;
  const ratios = usageRatios(m);
  const total = m.verification_total;
  const passPct = total > 0 ? (m.verification_passed / total) * 100 : 0;
  const failPct = total > 0 ? (m.verification_failed / total) * 100 : 0;
  const trim1 = (v: number) => String(Math.round(v * 10) / 10);
  return (
    <>
      <td className={styles.eventsCell}>
        {total > 0 ? (
          <span title={`passed ${m.verification_passed} · failed ${m.verification_failed} · unknown ${m.verification_unknown}`}>
            <strong>{m.verification_passed}</strong>/{total}
            <span
              style={{
                display: 'inline-block',
                width: 48,
                height: 5,
                borderRadius: 3,
                background: 'var(--wimcc-surface-2)',
                verticalAlign: 2,
                marginLeft: 8,
                overflow: 'hidden',
              }}
            >
              <i style={{ display: 'block', float: 'left', height: '100%', width: `${passPct}%`, background: '#41c285' }} />
              <i style={{ display: 'block', float: 'left', height: '100%', width: `${failPct}%`, background: '#ef4747' }} />
            </span>
          </span>
        ) : (
          <span className={styles.tagDim}>—</span>
        )}
      </td>
      <td className={styles.eventsCell}>{signalsOf(m) || <span className={styles.tagDim}>0</span>}</td>
      <td className={styles.eventsCell}>
        {ratios.measured ? `$${trim1(m.estimated_cost_usd ?? 0)}` : <span className={styles.tagDim}>—</span>}
      </td>
      <td className={styles.eventsCell}>
        {ratios.measured ? `$${trim1(ratios.unitRatePerM)}/1M` : <span className={styles.tagDim}>—</span>}
      </td>
      <td className={styles.eventsCell}>
        {ratios.measured ? `${trim1(ratios.cacheHitPct)}%` : <span className={styles.tagDim}>—</span>}
      </td>
    </>
  );
}

export default function SessionListPage() {
  const { locale } = useLocale();
  const t = useT();
  const [state, setState] = useState<State>({ kind: 'loading' });
  // 2026-07-04 리스트 지표 컬럼(추가형) — /v1/metrics join. 실패해도 기존
  // 컬럼은 그대로 렌더한다(지표만 '—').
  const [metrics, setMetrics] = useState<Map<string, SessionMetricsDto>>(() => new Map());
  const [sortKey, setSortKey] = useState<SortKey>('last_observed_at');
  const [sortDir, setSortDir] = useState<SortDir>('desc');
  const [query, setQuery] = useState('');
  const searchRef = useRef<HTMLInputElement>(null);

  // S10 (§7.4) — "/" focuses the search box (unless already typing in a field).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      const tag = t?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t?.isContentEditable) return;
      e.preventDefault();
      searchRef.current?.focus();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  // ticks every 5s so the live badge re-evaluates the 60s window and the
  // relative-time column re-renders even when no envelope arrives.
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNowMs(Date.now()), 5_000);
    return () => window.clearInterval(t);
  }, []);

  // slice-8 — per-session envelope arrival timestamps, populated by SSE.
  const [envelopeAt, setEnvelopeAt] = useState<Map<string, number>>(() => new Map());

  const load = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const rows = await listSessions();
      setState({ kind: 'ok', rows });
    } catch (e) {
      const message = e instanceof ApiError ? e.detail : String(e);
      setState({ kind: 'error', message });
    }
    // 기존 컬럼이 먼저 서고, 지표는 도착하는 대로 채운다 — 실패해도 '—' 유지.
    getMetricsSeries({ limit: 200 })
      .then((series) => {
        setMetrics(new Map(series.sessions.map((r) => [r.session_id, r.metrics])));
      })
      .catch(() => {
        /* 지표는 부가 컬럼 */
      });
  }, []);

  useEffect(() => { void load(); }, [load]);

  // slice-8 — subscribe to the global SSE stream. On each envelope, update
  // envelopeAt and mark the matching row's last_observed_at + event_count.
  const onEnvelope = useCallback(
    (env: LiveEnvelope) => {
      setEnvelopeAt((prev) => {
        const next = new Map(prev);
        next.set(env.session_id, Date.now());
        return next;
      });
      setState((s) => {
        if (s.kind !== 'ok') return s;
        const idx = s.rows.findIndex((r) => r.session_id === env.session_id);
        if (idx < 0) {
          // New session — refetch list to pull in the full row.
          void load();
          return s;
        }
        const updated = s.rows.slice();
        const row = updated[idx];
        updated[idx] = {
          ...row,
          last_observed_at: env.observed_at,
          event_count: row.event_count + 1,
        };
        return { kind: 'ok', rows: updated };
      });
    },
    [load],
  );
  useLiveStream({
    url: '/v1/stream',
    scope: 'global',
    onEnvelope,
    onResync: () => {
      setEnvelopeAt(new Map());
      void load();
    },
    onGap: () => {
      void load();
    },
  });

  const visibleRows = useMemo(() => {
    if (state.kind !== 'ok') return [];
    const copy = state.rows.filter((r) => matchesQuery(r, query));
    copy.sort((a, b) => {
      const c = compare(a, b, sortKey, metrics);
      return sortDir === 'asc' ? c : -c;
    });
    // teammate 세션(team_name 보유)을 리드 세션 바로 아래로 — 정렬 뒤에
    // 적용해 리드의 정렬 위치는 유지하고 팀메이트만 붙인다.
    return groupTeamRows(copy);
  }, [state, sortKey, sortDir, query, metrics]);

  const onHeaderClick = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDir(key === 'session_id' ? 'asc' : 'desc');
    }
  };

  const totalRows = state.kind === 'ok' ? state.rows.length : 0;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1>wimcc · Sessions</h1>
        <button type="button" onClick={() => void load()}>refresh</button>
      </header>
      {state.kind === 'loading' && <p>Loading…</p>}
      {state.kind === 'error' && (
        <div role="alert">
          <p>{state.message}</p>
          <button type="button" onClick={() => void load()}>Retry</button>
        </div>
      )}
      {state.kind === 'ok' && totalRows === 0 && (
        <div className={styles.empty}>
          <p>No sessions yet.</p>
          <p>Run <code>wimcc serve --auto-migrate</code> and let claude code talk to it, or backfill with <code>wimcc ingest --all</code>.</p>
        </div>
      )}
      {state.kind === 'ok' && totalRows > 0 && (
        <>
          <div className={styles.toolbar}>
            <p className={styles.hint}>
              {visibleRows.length} sessions · sorted by <strong>{SORT_LABELS[sortKey]}</strong>{' '}
              {sortDir === 'desc' ? '↓' : '↑'}
            </p>
            <input
              ref={searchRef}
              className={styles.search}
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('sessions.searchPlaceholder')}
              aria-label={t('sessions.searchAria')}
            />
          </div>
          <table className={styles.table}>
            <thead>
              <tr>
                <th onClick={() => onHeaderClick('session_id')} className={styles.sortable}>
                  session<SortIndicator active={sortKey === 'session_id'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('last_observed_at')} className={styles.sortable}>
                  last seen<SortIndicator active={sortKey === 'last_observed_at'} dir={sortDir} />
                </th>
                <th
                  onClick={() => onHeaderClick('span')}
                  className={`${styles.sortable} ${styles.numHead}`}
                  title="세션 span — 첫 관측 → 마지막 관측 (유휴 포함)"
                >
                  span<SortIndicator active={sortKey === 'span'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('event_count')} className={`${styles.sortable} ${styles.numHead}`}>
                  events<SortIndicator active={sortKey === 'event_count'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('verification')} className={`${styles.sortable} ${styles.numHead}`}>
                  verify<SortIndicator active={sortKey === 'verification'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('signals')} className={`${styles.sortable} ${styles.numHead}`}>
                  signals<SortIndicator active={sortKey === 'signals'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('cost')} className={`${styles.sortable} ${styles.numHead}`}>
                  cost<SortIndicator active={sortKey === 'cost'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('rate')} className={`${styles.sortable} ${styles.numHead}`}>
                  $/1M<SortIndicator active={sortKey === 'rate'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('hit')} className={`${styles.sortable} ${styles.numHead}`}>
                  hit<SortIndicator active={sortKey === 'hit'} dir={sortDir} />
                </th>
                <th>sources</th>
              </tr>
            </thead>
            <tbody>
              {visibleRows.map(({ row: r, child }) => {
                const mix = sourceMix(r.by_kind);
                const otelOnly = mix.transcript === 0 && mix.hook === 0 && mix.otel > 0;
                const live = isLive(envelopeAt.get(r.session_id), nowMs);
                const label = r.slug ?? r.session_id;
                const proj = projectBasename(r.project);
                return (
                  <tr key={r.session_id} className={otelOnly ? styles.otelOnly : undefined}>
                    <td className={child ? `${styles.sessionCell} ${styles.teamChild}` : styles.sessionCell}>
                      <div className={styles.top}>
                        {child && <span className={styles.childArrow} aria-hidden>↳</span>}
                        <Link to={`/sessions/${r.session_id}`} className={styles.slug} title={r.session_id}>
                          {label}
                        </Link>
                        {r.agent_name && (
                          <span
                            className={styles.agentChip}
                            style={{ color: agentColor(r.agent_name), borderColor: agentColor(r.agent_name) }}
                            title={r.team_name ?? undefined}
                          >
                            {r.agent_name}
                          </span>
                        )}
                        {proj && (
                          <span className={styles.proj} title={r.project}>{proj}</span>
                        )}
                        {live && (
                          <span
                            className={styles.liveBadge}
                            data-testid="live-badge"
                            title="received an SSE envelope within 60s — claude is currently active"
                          >
                            ● live
                          </span>
                        )}
                        {r.model && <span className={styles.model}>{formatModel(r.model)}</span>}
                      </div>
                      {r.first_user_message_preview && (
                        <div className={styles.prev}>{r.first_user_message_preview}</div>
                      )}
                    </td>
                    <td className={styles.relCell} title={r.last_observed_at}>
                      {relativeTime(r.last_observed_at, nowMs, locale)}
                    </td>
                    <td
                      className={styles.eventsCell}
                      title={`${r.first_observed_at} → ${r.last_observed_at}`}
                    >
                      {formatSpan(r.first_observed_at, r.last_observed_at)}
                    </td>
                    <td className={styles.eventsCell}>{r.event_count.toLocaleString()}</td>
                    <MetricCells m={metrics.get(r.session_id)} />
                    <td className={styles.mix}>
                      {mix.transcript > 0 && (
                        <span className={`${styles.tag} ${styles.tagTranscript}`}>txn {mix.transcript}</span>
                      )}
                      {mix.otel > 0 && (
                        <span className={`${styles.tag} ${styles.tagOtel}`}>otel {mix.otel}</span>
                      )}
                      {mix.hook > 0 && (
                        <span className={`${styles.tag} ${styles.tagHook}`}>hook {mix.hook}</span>
                      )}
                      {!r.by_kind && <span className={styles.tagDim}>—</span>}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
