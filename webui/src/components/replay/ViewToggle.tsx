/**
 * PR-7 — Waterfall ⇄ Graph toggle. Backed by URL search param `view`.
 * Default (no param) is waterfall.
 */
import { useSearchParams } from 'react-router-dom';
import styles from './ViewToggle.module.css';

export type ReplayViewMode = 'waterfall' | 'graph';

export function useReplayView(): ReplayViewMode {
  const [params] = useSearchParams();
  return params.get('view') === 'graph' ? 'graph' : 'waterfall';
}

export function ViewToggle() {
  const [params, setParams] = useSearchParams();
  const current = useReplayView();

  const set = (mode: ReplayViewMode) => {
    const next = new URLSearchParams(params);
    if (mode === 'waterfall') next.delete('view');
    else next.set('view', mode);
    setParams(next, { replace: true });
  };

  return (
    <div className={styles.toggle} role="group" aria-label="Replay view">
      <button
        type="button"
        className={styles.btn}
        aria-pressed={current === 'waterfall'}
        data-state={current === 'waterfall' ? 'active' : 'inactive'}
        onClick={() => set('waterfall')}
      >
        Waterfall
      </button>
      <button
        type="button"
        className={styles.btn}
        aria-pressed={current === 'graph'}
        data-state={current === 'graph' ? 'active' : 'inactive'}
        onClick={() => set('graph')}
      >
        Graph
      </button>
    </div>
  );
}
