/**
 * The redesigned insight surface (design spec §5 "Direction A").
 * Replaces the 6-tile KpiStrip. Compact cards organized around the five
 * diagnostic questions, each with a provenance badge and a `?` tooltip;
 * clicking a card toggles an inline drill-down (single-open). The removed
 * tiles (Risk / Episodes / Outcome / latency, spec §1/§5/§11 P1) are gone.
 *
 * All derivation is pure in `insightCards.ts`; this component only renders and
 * owns the expand state.
 */
import { useState } from 'react';
import type {
  SessionUsageDto,
  VerificationRunDto,
  SignalDto,
} from '../../../api/types';
import { buildInsightCards, type InsightBaseline, type InsightCardId } from './insightCards';
import { ProvenanceBadge } from './ProvenanceBadge';
import { InfoTip } from './InfoTip';
import styles from './InsightStrip.module.css';

interface InsightStripProps {
  usage: SessionUsageDto | undefined;
  verificationRuns: VerificationRunDto[] | undefined;
  signals: SignalDto[] | undefined;
  /** slice 6 — optional cross-session baseline; omitted today. */
  baseline?: InsightBaseline;
}

export function InsightStrip(props: InsightStripProps) {
  const cards = buildInsightCards({
    usage: props.usage,
    verificationRuns: props.verificationRuns,
    signals: props.signals,
    baseline: props.baseline,
  });
  const [openId, setOpenId] = useState<InsightCardId | null>(null);

  return (
    <section className={styles.strip} aria-label="세션 인사이트">
      <div className={styles.row}>
        {cards.map((card) => {
          const open = openId === card.id;
          return (
            <div
              key={card.id}
              className={styles.card}
              data-testid={`insight-card-${card.id}`}
              data-provenance={card.provenance}
              data-open={open}
            >
              <div className={styles.cardHead}>
                <span className={styles.cardTitle}>{card.title}</span>
                <InfoTip label={card.title} text={card.tooltip} />
              </div>
              <button
                type="button"
                className={styles.cardToggle}
                data-testid={`insight-card-${card.id}-toggle`}
                aria-expanded={open}
                onClick={() => setOpenId((cur) => (cur === card.id ? null : card.id))}
              >
                <span className={styles.cardValue}>{card.value}</span>
                <span className={styles.cardDetail}>{card.detail}</span>
              </button>
              <div className={styles.cardFoot}>
                <ProvenanceBadge provenance={card.provenance} />
                {card.baselineDelta && (
                  <span className={styles.baselineDelta}>{card.baselineDelta}</span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {cards.map((card) =>
        openId === card.id && card.drill ? (
          <div
            key={`drill-${card.id}`}
            className={styles.drill}
            data-testid={`insight-drill-${card.id}`}
          >
            <ul className={styles.drillList}>
              {card.drill.lines.map((line, i) => (
                <li key={i} className={styles.drillItem}>{line}</li>
              ))}
            </ul>
          </div>
        ) : null,
      )}
    </section>
  );
}
