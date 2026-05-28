/**
 * PR-3 — top-of-page KPI strip. Six compact tiles. Each tile has a stable
 * data-testid so PR-8 (responsive) and PR-6 (Why Panel cross-reference)
 * can target them without DOM churn.
 */
import { formatPct, formatUsd, formatMs } from '../../lib/format';
import styles from './KpiStrip.module.css';

export type KpiOutcome = 'clean' | 'attention' | 'problem' | 'unknown';

interface KpiStripProps {
  outcome: KpiOutcome;
  verificationCoverage: { covered: number; total: number } | null;
  episodeCount: number;
  riskCount: number;
  cost?: number;
  latencyP95Ms?: number;
}

function OutcomeTile({ outcome }: { outcome: KpiOutcome }) {
  const label =
    outcome === 'clean' ? 'Clean run' :
    outcome === 'attention' ? 'Needs attention' :
    outcome === 'problem' ? 'Problems' :
    'Unknown';
  return (
    <div
      className={styles.tile}
      data-testid="kpi-outcome"
      data-state={outcome}
      aria-label={`Outcome: ${label}`}
    >
      <span className={styles.tileLabel}>Outcome</span>
      <span className={styles.tileValue}>{label}</span>
    </div>
  );
}

function VerificationTile({ cov }: { cov: KpiStripProps['verificationCoverage'] }) {
  const ratio = cov && cov.total > 0 ? cov.covered / cov.total : null;
  return (
    <div className={styles.tile} data-testid="kpi-verification" aria-label="Verification coverage">
      <span className={styles.tileLabel}>Verification</span>
      <span className={styles.tileValue}>{formatPct(ratio)}</span>
      {cov && cov.total > 0 && (
        <span className={styles.tileSubtle}>{cov.covered} / {cov.total}</span>
      )}
    </div>
  );
}

export function KpiStrip(props: KpiStripProps) {
  const riskState = props.riskCount > 0 ? 'has-risk' : 'no-risk';
  return (
    <section className={styles.strip} aria-label="Session KPIs">
      <OutcomeTile outcome={props.outcome} />
      <VerificationTile cov={props.verificationCoverage} />
      <div className={styles.tile} data-testid="kpi-episodes" aria-label="Episode count">
        <span className={styles.tileLabel}>Episodes</span>
        <span className={styles.tileValue}>{props.episodeCount}</span>
      </div>
      <div
        className={styles.tile}
        data-testid="kpi-risk"
        data-state={riskState}
        aria-label={`Risk findings: ${props.riskCount}`}
      >
        <span className={styles.tileLabel}>Risk</span>
        <span className={styles.tileValue}>{props.riskCount}</span>
      </div>
      <div className={styles.tile} data-testid="kpi-cost" aria-label="Estimated cost">
        <span className={styles.tileLabel}>Cost</span>
        <span className={styles.tileValue}>{formatUsd(props.cost)}</span>
      </div>
      <div className={styles.tile} data-testid="kpi-latency" aria-label="Latency p95">
        <span className={styles.tileLabel}>Latency p95</span>
        <span className={styles.tileValue}>{formatMs(props.latencyP95Ms)}</span>
      </div>
    </section>
  );
}
