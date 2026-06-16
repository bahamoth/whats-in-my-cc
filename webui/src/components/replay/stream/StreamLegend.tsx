// S10 (UX 재설계 §7.4) — a dismissible colour/shortcut key for the stream.
// The time-spine gutter is decorative (aria-hidden), so this legend is the
// accessible key to the lane colours + duration heat, and documents the
// keyboard shortcuts. Dismissal is persisted (localStorage) so it stays out of
// the way once read.
import { useState } from 'react';
import { X } from 'lucide-react';
import { useT, type MessageKey } from '../../../i18n';
import styles from './StreamLegend.module.css';

const DISMISS_KEY = 'wimcc.streamLegend.dismissed';

const LANES: Array<{ labelKey: MessageKey; varName: string }> = [
  { labelKey: 'stream.lane.user', varName: '--wimcc-accent' },
  { labelKey: 'stream.lane.scaffold', varName: '--wimcc-scaffold' },
  { labelKey: 'stream.lane.tool', varName: '--wimcc-lane-action' },
  { labelKey: 'stream.lane.thinking', varName: '--wimcc-lane-context' },
  { labelKey: 'stream.lane.batch', varName: '--wimcc-lane-hook' },
  { labelKey: 'stream.lane.workflow', varName: '--wimcc-lane-quality' },
];

export function StreamLegend() {
  const t = useT();
  const [dismissed, setDismissed] = useState(() => {
    try {
      return localStorage.getItem(DISMISS_KEY) === '1';
    } catch {
      return false;
    }
  });
  if (dismissed) return null;

  const dismiss = () => {
    try {
      localStorage.setItem(DISMISS_KEY, '1');
    } catch {
      /* ignore quota / private-mode failures — dismissal is best-effort */
    }
    setDismissed(true);
  };

  return (
    <div className={styles.legend} role="note" aria-label={t('stream.legend.aria')}>
      <div className={styles.group}>
        {LANES.map((l) => (
          <span key={l.labelKey} className={styles.item}>
            <i className={styles.swatch} style={{ background: `var(${l.varName})` }} aria-hidden />
            {t(l.labelKey)}
          </span>
        ))}
      </div>
      <div className={styles.group}>
        <span className={styles.item}>
          <i className={`${styles.swatch} ${styles.heatWarn}`} aria-hidden /> ≥10s
        </span>
        <span className={styles.item}>
          <i className={`${styles.swatch} ${styles.heatHot}`} aria-hidden /> ≥60s
        </span>
      </div>
      <div className={styles.group}>
        <span className={styles.keys}>
          <kbd>j</kbd>/<kbd>k</kbd> {t('stream.legend.move')} · <kbd>e</kbd> {t('stream.legend.nextError')}
        </span>
      </div>
      <button type="button" className={styles.close} onClick={dismiss} aria-label={t('stream.legend.close')}>
        <X size={12} aria-hidden />
      </button>
    </div>
  );
}
