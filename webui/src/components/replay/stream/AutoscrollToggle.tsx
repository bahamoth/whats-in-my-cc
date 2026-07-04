// Autoscroll (follow-tail) control, rendered as a footer bar UNDER the
// conversation stream (outside the scroll area) so it never covers messages.
// Presentational only — all state/decisions live in useAutoscroll.
//   ON  : "자동 스크롤" + switch on   (following the live tip; click to stop)
//   OFF : "자동 스크롤" + switch off + "N ↓"  (detached; click to jump + follow)
import type { ReactNode } from 'react';
import { useT } from '../../../i18n';
import styles from './AutoscrollToggle.module.css';

interface AutoscrollToggleProps {
  autoscroll: boolean;
  newCount: number;
  /** OFF → click: jump to the bottom and resume following. */
  onEnable: () => void;
  /** ON → click: stop following. */
  onDisable: () => void;
  /** Optional content shown on the LEFT of the footer bar (e.g. the
   *  untagged-Bash control), so all stream-footer affordances live together. */
  leftSlot?: ReactNode;
  /** Filter active (§1.4): the pending count is over the FILTERED subset only,
   *  not the true arrival count, so it reads as approximate — show the plain
   *  "새 이벤트 ↓" label instead of a specific number. */
  indeterminate?: boolean;
}

export function AutoscrollToggle({ autoscroll, newCount, onEnable, onDisable, leftSlot, indeterminate }: AutoscrollToggleProps) {
  const t = useT();
  const showCount = (!autoscroll && newCount > 0) || (!!indeterminate && newCount > 0);
  return (
    <div className={styles.footer} role="status" aria-live="off">
      <div className={styles.left}>{leftSlot}</div>
      <button
        type="button"
        className={styles.toggle}
        data-on={autoscroll ? 'true' : 'false'}
        aria-pressed={autoscroll}
        aria-label={autoscroll ? t('stream.autoscroll.disableAria') : t('stream.autoscroll.enableAria')}
        onClick={autoscroll ? onDisable : onEnable}
      >
        <span className={styles.label}>{t('stream.autoscroll.label')}</span>
        <span className={styles.switch} data-on={autoscroll ? 'true' : 'false'} aria-hidden>
          <span className={styles.knob} />
        </span>
        {showCount && (
          <span className={styles.count} data-testid="autoscroll-new-count">
            {indeterminate ? t('stream.newEvents') : `${newCount} ↓`}
          </span>
        )}
      </button>
    </div>
  );
}
