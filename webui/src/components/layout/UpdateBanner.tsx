import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { getHealthVersion } from '../../api/client';
import type { HealthVersion } from '../../api/types';
import styles from './UpdateBanner.module.css';

/** 새 릴리스가 있을 때만 뜨는 시스템 배너 (Task 4의 health `version` 블록 소비).
 *  세션당 1회 닫기 가능 — 영속 없음(YAGNI). 판정 문장 없이 버전 숫자·사실만 표기. */
export function UpdateBanner() {
  const t = useT();
  const [version, setVersion] = useState<HealthVersion | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    void getHealthVersion().then(setVersion);
  }, []);

  if (dismissed || !version?.update_available || !version.latest) return null;
  return (
    <div className={styles.banner} role="status">
      <span>{t('update.available', { current: version.current, latest: version.latest })}</span>
      <a
        className={styles.link}
        href={`https://github.com/bahamoth/whats-in-my-cc/releases/tag/${version.latest}`}
        target="_blank"
        rel="noreferrer"
      >
        {t('update.releaseNotes')}
      </a>
      <button
        type="button"
        className={styles.dismiss}
        onClick={() => setDismissed(true)}
        aria-label={t('update.dismiss')}
      >
        ×
      </button>
    </div>
  );
}
