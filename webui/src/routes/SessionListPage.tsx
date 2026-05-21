import { useEffect, useMemo, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { listSessions, ApiError } from '../api/client';
import type { SessionListItem } from '../api/types';
import styles from './SessionListPage.module.css';

type SortKey = 'last_observed_at' | 'first_observed_at' | 'event_count' | 'session_id';
type SortDir = 'asc' | 'desc';

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
const FILE_GIT_KINDS = new Set(['file_event', 'git_commit', 'diff_hunk']);

type SourceMix = {
  transcript: number;
  otel: number;
  hook: number;
  file_git: number;
};

function sourceMix(byKind?: Record<string, number>): SourceMix {
  const mix: SourceMix = { transcript: 0, otel: 0, hook: 0, file_git: 0 };
  if (!byKind) return mix;
  for (const [k, v] of Object.entries(byKind)) {
    if (TRANSCRIPT_KINDS.has(k)) mix.transcript += v;
    else if (OTEL_KINDS.has(k)) mix.otel += v;
    else if (FILE_GIT_KINDS.has(k)) mix.file_git += v;
    else if (k === 'hook_event') mix.hook += v;
  }
  return mix;
}

function compare(a: SessionListItem, b: SessionListItem, key: SortKey): number {
  switch (key) {
    case 'event_count':
      return a.event_count - b.event_count;
    case 'session_id':
      return a.session_id.localeCompare(b.session_id);
    case 'first_observed_at':
      return a.first_observed_at.localeCompare(b.first_observed_at);
    case 'last_observed_at':
    default:
      return a.last_observed_at.localeCompare(b.last_observed_at);
  }
}

function SortIndicator({ active, dir }: { active: boolean; dir: SortDir }) {
  if (!active) return <span className={styles.sortInactive}> ·</span>;
  return <span className={styles.sortActive}> {dir === 'desc' ? '▼' : '▲'}</span>;
}

export default function SessionListPage() {
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [sortKey, setSortKey] = useState<SortKey>('last_observed_at');
  const [sortDir, setSortDir] = useState<SortDir>('desc');

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

  const sortedRows = useMemo(() => {
    if (state.kind !== 'ok') return [];
    const copy = state.rows.slice();
    copy.sort((a, b) => {
      const c = compare(a, b, sortKey);
      return sortDir === 'asc' ? c : -c;
    });
    return copy;
  }, [state, sortKey, sortDir]);

  const onHeaderClick = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDir(key === 'session_id' ? 'asc' : 'desc');
    }
  };

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1>witmcc · Sessions</h1>
        <button type="button" onClick={() => void load()}>refresh</button>
      </header>
      {state.kind === 'loading' && <p>Loading…</p>}
      {state.kind === 'error' && (
        <div role="alert">
          <p>{state.message}</p>
          <button type="button" onClick={() => void load()}>Retry</button>
        </div>
      )}
      {state.kind === 'ok' && state.rows.length === 0 && (
        <div className={styles.empty}>
          <p>No sessions yet.</p>
          <p>Run <code>witmcc serve --auto-migrate</code> and let claude code talk to it, or backfill with <code>witmcc ingest --all</code>.</p>
        </div>
      )}
      {state.kind === 'ok' && state.rows.length > 0 && (
        <>
          <p className={styles.hint}>
            {sortedRows.length} sessions · sorted by <strong>{sortKey}</strong> {sortDir === 'desc' ? '↓' : '↑'} · click a column header to change
          </p>
          <table className={styles.table}>
            <thead>
              <tr>
                <th onClick={() => onHeaderClick('session_id')} className={styles.sortable}>
                  session_id<SortIndicator active={sortKey === 'session_id'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('first_observed_at')} className={styles.sortable}>
                  first_observed_at<SortIndicator active={sortKey === 'first_observed_at'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('last_observed_at')} className={styles.sortable}>
                  last_observed_at<SortIndicator active={sortKey === 'last_observed_at'} dir={sortDir} />
                </th>
                <th onClick={() => onHeaderClick('event_count')} className={styles.sortable}>
                  events<SortIndicator active={sortKey === 'event_count'} dir={sortDir} />
                </th>
                <th>sources</th>
              </tr>
            </thead>
            <tbody>
              {sortedRows.map((r) => {
                const mix = sourceMix(r.by_kind);
                const otelOnly =
                  mix.transcript === 0 && mix.hook === 0 && mix.file_git === 0 && mix.otel > 0;
                return (
                  <tr key={r.session_id} className={otelOnly ? styles.otelOnly : undefined}>
                    <td><Link to={`/sessions/${r.session_id}`}>{r.session_id}</Link></td>
                    <td>{r.first_observed_at}</td>
                    <td>{r.last_observed_at}</td>
                    <td>{r.event_count}</td>
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
                      {mix.file_git > 0 && (
                        <span className={`${styles.tag} ${styles.tagFile}`}>file {mix.file_git}</span>
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
