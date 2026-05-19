import { useEffect, useState } from 'react';
import { ApiError, getEventRaw } from '../api/client';
import type { RawEventResponse } from '../api/types';
import { JsonView } from './JsonView';
import styles from './SourcePanel.module.css';

type Props = { eventId: string | null };

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ok'; data: RawEventResponse }
  | { kind: 'error'; status: number; message: string };

export function SourcePanel({ eventId }: Props) {
  const [state, setState] = useState<State>(eventId ? { kind: 'loading' } : { kind: 'idle' });

  useEffect(() => {
    if (!eventId) { setState({ kind: 'idle' }); return; }
    let cancelled = false;
    setState({ kind: 'loading' });
    getEventRaw(eventId)
      .then((data) => { if (!cancelled) setState({ kind: 'ok', data }); })
      .catch((e: unknown) => {
        if (cancelled) return;
        if (e instanceof ApiError) setState({ kind: 'error', status: e.status, message: e.detail });
        else setState({ kind: 'error', status: 0, message: String(e) });
      });
    return () => { cancelled = true; };
  }, [eventId]);

  return (
    <aside className={styles.panel}>
      {state.kind === 'idle' && <p className={styles.hint}>Click a node to see its source record.</p>}
      {state.kind === 'loading' && <p>Loading raw record…</p>}
      {state.kind === 'error' && state.status === 404 && (
        <p className={styles.hint}>raw record not available for this event</p>
      )}
      {state.kind === 'error' && state.status === 410 && (
        <p className={styles.hint}>raw record pruned by retention</p>
      )}
      {state.kind === 'error' && state.status !== 404 && state.status !== 410 && (
        <p role="alert">Error: {state.message}</p>
      )}
      {state.kind === 'ok' && (
        <>
          <header className={styles.header}>
            <span className={styles.type}>{state.data.record_type}</span>
            <span className={styles.source}>
              <span>{state.data.source.file_path}</span>
              <span>:{state.data.source.line_no}</span>
            </span>
          </header>
          <div className={styles.body}>
            <JsonView data={state.data.record} />
          </div>
        </>
      )}
    </aside>
  );
}
