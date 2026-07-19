import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { getHealthVersion } from '../../api/client';
import type { HealthVersion } from '../../api/types';
import styles from './UpdateBanner.module.css';

/** 새 릴리스가 있을 때만 뜨는 시스템 배너 (health `version` 블록 소비).
 *  세션당 1회 닫기 가능 — 영속 없음(YAGNI). 판정 문장 없이 버전 숫자·사실만 표기.
 *
 *  2026-07-19 auto-update — 상태별 다음 행동을 함께 표기한다:
 *  - `downloaded` 존재: 바이너리 교체 완료, `wimcc service restart`로 적용
 *  - shell 채널: `wimcc self-update`
 *  - managed 채널(brew/cargo): 패키지 매니저 명령
 *  - 채널 미판별(구 서버): 릴리스 노트 링크만 (기존 동작) */
export function UpdateBanner() {
  const t = useT();
  const [version, setVersion] = useState<HealthVersion | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    void getHealthVersion().then(setVersion);
  }, []);

  if (dismissed || !version) return null;
  const downloaded = version.downloaded ?? null;
  if (!downloaded && (!version.update_available || !version.latest)) return null;
  const tag = downloaded ?? version.latest ?? '';

  return (
    <div className={styles.banner} role="status">
      {downloaded ? (
        <>
          <span>{t('update.ready', { tag: downloaded })}</span>
          <code className={styles.cmd}>wimcc service restart</code>
        </>
      ) : (
        <>
          <span>
            {t('update.available', { current: version.current, latest: version.latest ?? '' })}
          </span>
          {version.install_channel === 'managed' && (
            <code className={styles.cmd}>brew upgrade bahamoth/tap/wimcc</code>
          )}
          {version.install_channel === 'shell' && (
            <code className={styles.cmd}>wimcc self-update</code>
          )}
        </>
      )}
      <a
        className={styles.link}
        href={`https://github.com/bahamoth/whats-in-my-cc/releases/tag/${tag}`}
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
