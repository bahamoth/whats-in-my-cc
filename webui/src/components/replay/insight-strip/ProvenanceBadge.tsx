/**
 * slice-7 — provenance badge. A small inline pill that states the trust level
 * of the value next to it (design spec §2 P3, §5).
 */
import { type Provenance, PROVENANCE_LABEL } from './provenance';
import styles from './ProvenanceBadge.module.css';

export function ProvenanceBadge({ provenance }: { provenance: Provenance }) {
  return (
    <span
      className={styles.badge}
      data-testid="provenance-badge"
      data-provenance={provenance}
    >
      {PROVENANCE_LABEL[provenance]}
    </span>
  );
}
