// webui/src/components/replay/detail/InsightTab.tsx
import type { FindingDto } from '../../../api/types';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  findings: FindingDto[];
}

const SEV_CLASS: Record<string, string> = { high: 'sevHigh', medium: 'sevMed', low: 'sevLow' };

export function InsightTab({ findings }: InsightTabProps) {
  if (findings.length === 0) {
    return <p className={styles.empty}>No insights for this node.</p>;
  }
  return (
    <ul className={styles.list}>
      {findings.map((f) => (
        <li key={f.finding_id} className={styles.item}>
          <div className={styles.head}>
            <span className={`${styles.sev} ${styles[SEV_CLASS[f.severity] ?? 'sevLow']}`}>{f.severity}</span>
            <span className={styles.category}>{f.category}</span>
            <span className={styles.confidence}>{Math.round(f.confidence * 100)}%</span>
          </div>
          <p className={styles.summary}>{f.summary}</p>
        </li>
      ))}
    </ul>
  );
}
