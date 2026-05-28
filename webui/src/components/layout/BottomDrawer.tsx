/**
 * PR-5 — slide-up drawer for raw JSON deep-dive. When closed it renders
 * NOTHING — that is a load-time invariant tested by BottomDrawer.test.tsx
 * (raw JSON trees can be heavy; we never pay the cost until the user
 * actually opens the drawer). When open it renders inside a portal-style
 * fixed container with a backdrop; Escape and backdrop-click close it.
 */
import { useEffect, useId, type ReactNode } from 'react';
import styles from './BottomDrawer.module.css';

interface BottomDrawerProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
}

export function BottomDrawer({ open, onClose, title, children }: BottomDrawerProps) {
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className={styles.root} data-testid="bottom-drawer" data-state="open">
      <div
        className={styles.backdrop}
        data-testid="bottom-drawer-backdrop"
        onClick={onClose}
      />
      <div
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className={styles.header}>
          <h2 id={titleId} className={styles.title}>{title}</h2>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Close raw drawer"
          >
            ×
          </button>
        </header>
        <div className={styles.body}>{children}</div>
      </div>
    </div>
  );
}
