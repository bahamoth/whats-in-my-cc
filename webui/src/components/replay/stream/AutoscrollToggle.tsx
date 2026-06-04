// Autoscroll (follow-tail) control, rendered as a footer bar UNDER the
// conversation stream (outside the scroll area) so it never covers messages.
// Presentational only — all state/decisions live in useAutoscroll.
//   ON  : "자동 스크롤" + switch on   (following the live tip; click to stop)
//   OFF : "자동 스크롤" + switch off + "N ↓"  (detached; click to jump + follow)
import styles from './AutoscrollToggle.module.css';

interface AutoscrollToggleProps {
  autoscroll: boolean;
  newCount: number;
  /** OFF → click: jump to the bottom and resume following. */
  onEnable: () => void;
  /** ON → click: stop following. */
  onDisable: () => void;
}

export function AutoscrollToggle({ autoscroll, newCount, onEnable, onDisable }: AutoscrollToggleProps) {
  const showCount = !autoscroll && newCount > 0;
  return (
    <div className={styles.footer} role="status" aria-live="off">
      <button
        type="button"
        className={styles.toggle}
        data-on={autoscroll ? 'true' : 'false'}
        aria-pressed={autoscroll}
        aria-label={autoscroll ? '자동 스크롤 끄기' : '자동 스크롤 켜고 최신으로 이동'}
        onClick={autoscroll ? onDisable : onEnable}
      >
        <span className={styles.label}>자동 스크롤</span>
        <span className={styles.switch} data-on={autoscroll ? 'true' : 'false'} aria-hidden>
          <span className={styles.knob} />
        </span>
        {showCount && (
          <span className={styles.count} data-testid="autoscroll-new-count">
            {newCount} ↓
          </span>
        )}
      </button>
    </div>
  );
}
