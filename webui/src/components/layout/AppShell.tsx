import { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import styles from './AppShell.module.css';

interface AppShellProps {
  children: ReactNode;
  rightSlot?: ReactNode;
}

export function AppShell({ children, rightSlot }: AppShellProps) {
  return (
    <div className={styles.shell} data-witmcc-shell data-layout="grid">
      <a href="#witmcc-main" className={styles.skipLink}>
        Skip to content
      </a>
      <nav className={styles.navRail} aria-label="Primary">
        <Link to="/sessions" className={styles.navLink} aria-label="Sessions">
          <span aria-hidden="true" className={styles.navGlyph}>
            ◐
          </span>
          <span className={styles.navText}>Sessions</span>
        </Link>
      </nav>
      <main id="witmcc-main" className={styles.main} role="main">
        {children}
      </main>
      <aside
        className={styles.rightSlot}
        role="complementary"
        aria-label="Side panel"
      >
        {rightSlot}
      </aside>
    </div>
  );
}
