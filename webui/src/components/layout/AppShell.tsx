import { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { useT } from '../../i18n';
import { LanguageToggle } from './LanguageToggle';
import { ServeStatus } from './ServeStatus';
import { UpdateBanner } from './UpdateBanner';
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
        {/* 레일 푸터 — 남는 공간을 한 번만 흡수(margin-top:auto)해 토글·상태
            칩이 함께 하단에 정착한다. auto 마진이 두 요소에 나뉘면 토글이
            레일 중간에 뜬다(2026-07-19 사용자 지적). */}
        <div className={styles.railFooter}>
          <LanguageToggle />
          <ServeStatus />
        </div>
      </nav>
      <main id="wimcc-main" className={styles.main} role="main">
        <UpdateBanner />
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
