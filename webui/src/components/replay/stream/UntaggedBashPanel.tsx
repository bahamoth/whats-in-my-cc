// webui/src/components/replay/stream/UntaggedBashPanel.tsx
import { useMemo, useState } from 'react';
import type { ObservedEventDto } from '../../../api/types';
import { collectUntagged } from './eventTags';
import styles from './UntaggedBashPanel.module.css';

export function UntaggedBashPanel({ events }: { events: ObservedEventDto[] }) {
  const [open, setOpen] = useState(false);
  const rows = useMemo(() => collectUntagged(events), [events]);
  return (
    <div className={styles.wrap}>
      <button data-testid="untagged-toggle" className={styles.toggle} onClick={() => setOpen((v) => !v)}>
        untagged Bash {rows.length > 0 ? `(${rows.length})` : ''}
      </button>
      {open && (
        <div data-testid="untagged-list" className={styles.list}>
          {rows.length === 0 ? (
            <div className={styles.empty}>all Bash patterns tagged</div>
          ) : rows.map((r) => (
            <div key={r.token} className={styles.row}>
              <code className={styles.token}>{r.token}</code>
              <span className={styles.count}>×{r.count}</span>
              <code className={styles.sample}>{r.sample}</code>
              <span className={styles.hint}>{r.hint}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
