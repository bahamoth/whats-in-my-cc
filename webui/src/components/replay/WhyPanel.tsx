/**
 * PR-6 — WhyPanel renders a finding's evidence-linked explanation.
 *
 * Required props are intentionally explicit (finding, evidence?, open,
 * onClose, onEvidenceHover) so PR-7's CausalGraph can mount the exact
 * same panel without rewiring.
 */
import { useEffect } from 'react';
import { formatPct } from '../../lib/format';
import type {
  FindingDto,
  FindingEvidenceResponse,
  EvidenceRef,
} from '../../api/types';
import styles from './WhyPanel.module.css';

interface WhyPanelProps {
  open: boolean;
  finding: FindingDto | null;
  evidence?: FindingEvidenceResponse;
  onClose: () => void;
  onEvidenceHover: (nodeId: string | null) => void;
}

function evidenceNodeId(e: EvidenceRef): string | null {
  // Backend slice-14 serialises evidence_refs as bare ULID strings; the
  // judge variants emit `{ kind, node_id, ... }` objects.
  if (typeof e === 'string') return e;
  if (e.kind === 'node' && typeof e.node_id === 'string') return e.node_id;
  if (e.kind === 'edge' && typeof e.edge_id === 'string') return e.edge_id;
  if (e.kind === 'event' && typeof e.event_id === 'string') return e.event_id;
  return null;
}

function evidenceLabel(e: EvidenceRef): string {
  if (typeof e === 'string') return `evt:${e.slice(0, 12)}`;
  const id = evidenceNodeId(e) ?? '';
  return `${e.kind}:${id.slice(0, 12)}`;
}

export function WhyPanel({ open, finding, evidence, onClose, onEvidenceHover }: WhyPanelProps) {
  // Esc closes the panel. Effect is unconditional so the listener
  // tears down even if the parent flips `open` to false.
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  if (!open || !finding) return null;

  const noEvidence = !finding.evidence_refs || finding.evidence_refs.length === 0;
  const resourceUri = `whats-in-my-cc://findings/${finding.finding_id}`;

  const handleCopy = () => {
    void navigator.clipboard?.writeText?.(resourceUri);
  };

  return (
    <aside
      className={styles.panel}
      data-testid="why-panel"
      data-severity={finding.severity}
      data-category={finding.category}
      data-warning={noEvidence ? 'no-evidence' : undefined}
      role="region"
      aria-label="Why panel"
    >
      <header className={styles.header}>
        <span className={styles.category}>{finding.category}</span>
        <span className={styles.severity} data-state={finding.severity}>
          {finding.severity}
        </span>
        <button
          type="button"
          className={styles.closeBtn}
          onClick={onClose}
          aria-label="Close why panel"
        >
          ×
        </button>
      </header>
      <p className={styles.claim}>{finding.summary}</p>
      <div className={styles.confidenceRow}>
        <span className={styles.confidenceLabel}>Confidence</span>
        <span className={styles.confidenceValue}>{formatPct(finding.confidence)}</span>
      </div>
      <section aria-label="Evidence" className={styles.section}>
        <h3 className={styles.sectionTitle}>Evidence</h3>
        {noEvidence ? (
          <p className={styles.warning}>
            ⚠ No evidence references — this finding cannot be trusted.
          </p>
        ) : (
          <ul className={styles.chips}>
            {finding.evidence_refs.map((e, idx) => {
              const id = evidenceNodeId(e);
              if (!id) return null;
              return (
                <li key={`${id}-${idx}`}>
                  <button
                    type="button"
                    data-testid={`evidence-chip-${id}`}
                    className={styles.chip}
                    onMouseEnter={() => onEvidenceHover(id)}
                    onMouseLeave={() => onEvidenceHover(null)}
                  >
                    {evidenceLabel(e)}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>
      {evidence && evidence.raw_source_refs.length > 0 && (
        <section aria-label="Raw source refs" className={styles.section}>
          <h3 className={styles.sectionTitle}>Raw sources</h3>
          <ul className={styles.rawList}>
            {evidence.raw_source_refs.map((r) => (
              <li key={r.event_id} className={styles.rawItem}>
                <span className={styles.rawType}>{r.source_type}</span>
                <code className={styles.rawUri}>{r.source_uri}</code>
              </li>
            ))}
          </ul>
        </section>
      )}
      <section aria-label="Resource URI" className={styles.section}>
        <h3 className={styles.sectionTitle}>Resource URI</h3>
        <div className={styles.uriRow}>
          <code className={styles.uri}>{resourceUri}</code>
          <button type="button" className={styles.copyBtn} onClick={handleCopy}>
            Copy resource URI
          </button>
        </div>
      </section>
    </aside>
  );
}
