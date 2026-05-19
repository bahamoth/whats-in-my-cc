import { useEffect, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { listSessions, ApiError } from '../api/client';
import type { SessionListItem } from '../api/types';
import styles from './SessionListPage.module.css';

type State =
  | { kind: 'loading' }
  | { kind: 'ok'; rows: SessionListItem[] }
  | { kind: 'error'; message: string };

export default function SessionListPage() {
  const [state, setState] = useState<State>({ kind: 'loading' });

  const load = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const rows = await listSessions();
      rows.sort((a, b) => b.last_observed_at.localeCompare(a.last_observed_at));
      setState({ kind: 'ok', rows });
    } catch (e) {
      const message = e instanceof ApiError ? e.detail : String(e);
      setState({ kind: 'error', message });
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

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
          <p>Run <code>witmcc ingest --all</code> to start.</p>
        </div>
      )}
      {state.kind === 'ok' && state.rows.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>session_id</th>
              <th>first_observed_at</th>
              <th>last_observed_at</th>
              <th>events</th>
            </tr>
          </thead>
          <tbody>
            {state.rows.map((r) => (
              <tr key={r.session_id}>
                <td><Link to={`/sessions/${r.session_id}`}>{r.session_id}</Link></td>
                <td>{r.first_observed_at}</td>
                <td>{r.last_observed_at}</td>
                <td>{r.event_count}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
