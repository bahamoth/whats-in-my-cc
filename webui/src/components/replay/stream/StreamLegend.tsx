// S10 (UX 재설계 §7.4) — a dismissible colour/shortcut key for the stream.
// The time-spine gutter is decorative (aria-hidden), so this legend is the
// accessible key to the lane colours + duration heat, and documents the
// keyboard shortcuts. Dismissal is persisted (localStorage) so it stays out of
// the way once read.
import { useState } from 'react';
import { X } from 'lucide-react';
import styles from './StreamLegend.module.css';

const DISMISS_KEY = 'wimcc.streamLegend.dismissed';

const LANES: Array<{ label: string; varName: string }> = [
  { label: '사용자', varName: '--wimcc-accent' },
  { label: '스캐폴드', varName: '--wimcc-scaffold' },
  { label: '도구', varName: '--wimcc-lane-action' },
  { label: '추론', varName: '--wimcc-lane-context' },
  { label: '배치', varName: '--wimcc-lane-hook' },
  { label: '워크플로우', varName: '--wimcc-lane-quality' },
];

export function StreamLegend() {
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
    <div className={styles.legend} role="note" aria-label="스트림 범례">
      <div className={styles.group}>
        {LANES.map((l) => (
          <span key={l.label} className={styles.item}>
            <i className={styles.swatch} style={{ background: `var(${l.varName})` }} aria-hidden />
            {l.label}
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
          <kbd>j</kbd>/<kbd>k</kbd> 이동 · <kbd>e</kbd> 다음 오류
        </span>
      </div>
      <button type="button" className={styles.close} onClick={dismiss} aria-label="범례 닫기">
        <X size={12} aria-hidden />
      </button>
    </div>
  );
}
