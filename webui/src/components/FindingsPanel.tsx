import { useEffect, useState } from 'react';
import { ApiError, getFindings } from '../api/client';
import type { FindingDto } from '../api/types';
import styles from './FindingsPanel.module.css';

// Slice-11 — M5 Insight engine surface. Reads `/v1/sessions/:id/findings`
// and lists tool_failure (and future) rows. "Show evidence" routes the
// first evidence_refs.node_id back up to the parent so the Timeline can
// highlight it and SourcePanel can render its source.

type Props = {
  sessionId: string;
  onSelectNode: (nodeId: string) => void;
};

type State =
  | { kind: 'loading' }
  | { kind: 'ok'; findings: FindingDto[] }
  | { kind: 'error'; message: string };

function severityClass(sev: string): string {
  switch (sev) {
    case 'high':
      return styles.sevHigh;
    case 'low':
      return styles.sevLow;
    default:
      return styles.sevMedium;
  }
}

export function FindingsPanel({ sessionId, onSelectNode }: Props) {
  const [state, setState] = useState<State>({ kind: 'loading' });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: 'loading' });
    getFindings(sessionId)
      .then((data) => {
        if (!cancelled) setState({ kind: 'ok', findings: data.findings });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        const msg = e instanceof ApiError ? e.detail : String(e);
        setState({ kind: 'error', message: msg });
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  return (
    <section className={styles.panel} aria-labelledby="findings-heading">
      <div className={styles.header}>
        <span id="findings-heading" className={styles.title}>
          Findings
        </span>
        {state.kind === 'ok' && (
          <span className={styles.count}>{state.findings.length}</span>
        )}
      </div>

      {state.kind === 'loading' && <p className={styles.hint}>Loading…</p>}

      {state.kind === 'error' && (
        <p role="alert">Failed to load findings: {state.message}</p>
      )}

      {state.kind === 'ok' && state.findings.length === 0 && (
        <p className={styles.hint}>no findings for this session yet</p>
      )}

      {state.kind === 'ok' && state.findings.length > 0 && (
        <div className={styles.list}>
          {state.findings.map((f) => {
            const firstNode = f.evidence_refs[0]?.node_id ?? null;
            const confPct = Math.round(f.confidence * 100);
            return (
              <div key={f.finding_id} className={styles.row} data-finding-id={f.finding_id}>
                <div className={styles.rowHead}>
                  <span className={styles.cat}>{f.category}</span>
                  <span className={`${styles.sev} ${severityClass(f.severity)}`}>
                    {f.severity}
                  </span>
                  <span className={styles.conf}>{confPct}% confidence</span>
                </div>
                <p className={styles.claim}>{f.claim}</p>
                {f.limitations.length > 0 && (
                  <p className={styles.lim}>{f.limitations.join(' · ')}</p>
                )}
                <div className={styles.actions}>
                  <button
                    type="button"
                    className={styles.btn}
                    disabled={firstNode === null}
                    onClick={() => firstNode && onSelectNode(firstNode)}
                  >
                    show evidence
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
