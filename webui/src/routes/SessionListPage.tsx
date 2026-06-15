import { useEffect, useMemo, useRef, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { listSessions, ApiError } from '../api/client';
import type { SessionListItem } from '../api/types';
import { useLiveStream, type LiveEnvelope } from '../hooks/useLiveStream';
import { relativeTime, formatModel } from '../lib/format';
import styles from './SessionListPage.module.css';

type SortKey = 'last_observed_at' | 'event_count' | 'session_id';
type SortDir = 'asc' | 'desc';

const SORT_LABELS: Record<SortKey, string> = {
  session_id: 'session',
  last_observed_at: 'last seen',
  event_count: 'events',
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

function compare(a: SessionListItem, b: SessionListItem, key: SortKey): number {
  switch (key) {
    case 'event_count':
      return a.event_count - b.event_count;
    case 'session_id':
      return (a.slug ?? a.session_id).localeCompare(b.slug ?? b.session_id);
    case 'last_observed_at':
    default:
      return a.last_observed_at.localeCompare(b.last_observed_at);
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
  return [r.slug, r.project, r.session_id, r.first_user_message_preview, r.model]
    .some((f) => f?.toLowerCase().includes(needle));
}

export default function SessionListPage() {
  const [state, setState] = useState<State>({ kind: 'loading' });
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
      const c = compare(a, b, sortKey);
      return sortDir === 'asc' ? c : -c;
    });
    return copy;
  }, [state, sortKey, sortDir, query]);

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
              placeholder="⌕ 프로젝트·슬러그 검색… ( / )"
              aria-label="세션 검색"
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
                <th onClick={() => onHeaderClick('event_count')} className={`${styles.sortable} ${styles.numHead}`}>
                  events<SortIndicator active={sortKey === 'event_count'} dir={sortDir} />
                </th>
                <th>sources</th>
              </tr>
            </thead>
            <tbody>
              {visibleRows.map((r) => {
                const mix = sourceMix(r.by_kind);
                const otelOnly = mix.transcript === 0 && mix.hook === 0 && mix.otel > 0;
                const live = isLive(envelopeAt.get(r.session_id), nowMs);
                const label = r.slug ?? r.session_id;
                const proj = projectBasename(r.project);
                return (
                  <tr key={r.session_id} className={otelOnly ? styles.otelOnly : undefined}>
                    <td className={styles.sessionCell}>
                      <div className={styles.top}>
                        <Link to={`/sessions/${r.session_id}`} className={styles.slug} title={r.session_id}>
                          {label}
                        </Link>
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
                      {relativeTime(r.last_observed_at, nowMs)}
                    </td>
                    <td className={styles.eventsCell}>{r.event_count.toLocaleString()}</td>
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
