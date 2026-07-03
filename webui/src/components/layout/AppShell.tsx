import { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { useT } from '../../i18n';
import { LanguageToggle } from './LanguageToggle';
import styles from './AppShell.module.css';

interface AppShellProps {
  children: ReactNode;
  rightSlot?: ReactNode;
}

export function AppShell({ children, rightSlot }: AppShellProps) {
  const t = useT();
  return (
    <div className={styles.shell} data-wimcc-shell data-layout="grid">
      <a href="#wimcc-main" className={styles.skipLink}>
        {t('a11y.skipToContent')}
      </a>
      <nav className={styles.navRail} aria-label={t('a11y.primaryNav')}>
        <Link to="/sessions" className={styles.navLink} aria-label={t('nav.sessions')}>
          <span aria-hidden="true" className={styles.navGlyph}>
            ◐
          </span>
          <span className={styles.navText}>{t('nav.sessions')}</span>
        </Link>
        <Link to="/dashboard" className={styles.navLink} aria-label={t('nav.dashboard')}>
          <span aria-hidden="true" className={styles.navGlyph}>
            ▦
          </span>
          <span className={styles.navText}>{t('nav.dashboard')}</span>
        </Link>
        <LanguageToggle />
      </nav>
      <main id="wimcc-main" className={styles.main} role="main">
        {children}
      </main>
      <aside
        className={styles.rightSlot}
        role="complementary"
        aria-label={t('a11y.sidePanel')}
      >
        {rightSlot}
      </aside>
    </div>
  );
}
