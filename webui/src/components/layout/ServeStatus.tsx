import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { getHealthStatus } from '../../api/client';
import type { HealthStatus } from '../../api/types';
import { formatBytes } from '../../lib/format';
import { renderTipMarkup } from '../replay/insight-strip/InfoTip';
import styles from './ServeStatus.module.css';

const POLL_MS = 60_000;

/** growth-2026-07-18 — nav rail 하단 serve 상태 칩: 실행 중 버전, 최신 릴리스
 *  관측 마커, DB 용량. hover 패널이 경로·회수 가능 용량·retention·마지막
 *  sweep을 보탠다. 판정 문장 없이 숫자·관측 사실만(WebUI 표기 원칙);
 *  health 실패 시 조용히 생략(UpdateBanner와 동일한 fail-soft 계약). */
export function ServeStatus() {
  const t = useT();
  const [health, setHealth] = useState<HealthStatus | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let alive = true;
    const pull = () =>
      void getHealthStatus().then((h) => {
        if (alive) setHealth(h);
      });
    pull();
    const id = window.setInterval(pull, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  if (!health) return null;
  const { version, db, security, retention } = health;

  return (
    <div
      className={styles.wrap}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      <button type="button" className={styles.chip} aria-label={t('status.aria')}>
        <span className={styles.version}>
          v{version.current}
          {version.update_available && version.latest && (
            <i className={styles.updateDot} data-testid="update-marker" aria-hidden="true" />
          )}
        </span>
        <span className={styles.size}>{formatBytes(db.size_bytes)}</span>
      </button>
      {open && (
        <div className={styles.panel} role="tooltip">
          <dl className={styles.rows}>
            <div className={styles.row}>
              <dt>{t('status.version')}</dt>
              <dd>v{version.current}</dd>
            </div>
            {version.latest && (
              <div className={styles.row}>
                <dt>{t('status.latest')}</dt>
                <dd>{version.latest}</dd>
              </div>
            )}
            <div className={styles.row}>
              <dt>{t('status.dbSize')}</dt>
              <dd>{formatBytes(db.size_bytes)}</dd>
            </div>
            <div className={styles.row}>
              <dt>{t('status.freelist')}</dt>
              <dd>{formatBytes(db.freelist_bytes)}</dd>
            </div>
            {db.path && (
              <div className={styles.row}>
                <dt>{t('status.dbPath')}</dt>
                <dd className={styles.path}>{db.path}</dd>
              </div>
            )}
            <div className={styles.row}>
              <dt>{t('status.retention')}</dt>
              <dd>{security.retention_profile}</dd>
            </div>
            <div className={styles.row}>
              <dt>{t('status.lastSweep')}</dt>
              {/* 미측정 ≠ 0 — sweep 미실행은 '—' */}
              <dd>{retention.last_sweep_at ?? '—'}</dd>
            </div>
          </dl>
          <p className={styles.tipText}>{renderTipMarkup(t('status.serve.tip'))}</p>
        </div>
      )}
    </div>
  );
}
