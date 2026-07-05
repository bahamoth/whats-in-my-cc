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
  TurnRollupDto,
} from '../../../api/types';
import {
  buildInsightCards,
  type InsightBaseline,
  type InsightCardId,
  type SparklineTint,
} from './insightCards';
import { ProvenanceBadge } from './ProvenanceBadge';
import { InfoTip } from './InfoTip';
import { useT } from '../../../i18n';
import styles from './InsightStrip.module.css';

interface InsightStripProps {
  usage: SessionUsageDto | undefined;
  verificationRuns: VerificationRunDto[] | undefined;
  signals: SignalDto[] | undefined;
  /** slice 6 — optional cross-session baseline; omitted today. */
  baseline?: InsightBaseline;
  /** S8 — per-turn rollup for intra-session sparklines. */
  turns?: TurnRollupDto[];
}

/** S8 — a compact bar sparkline. Values are normalised to the series max so the
 *  trend shape reads at a glance; an all-zero series renders flat. */
function Sparkline({ values, tint }: { values: number[]; tint?: SparklineTint }) {
  const max = Math.max(...values, 0);
  return (
    <div className={styles.spark} data-testid="sparkline" data-tint={tint ?? 'blue'} aria-hidden>
      {values.map((v, i) => (
        <i key={i} data-bar style={{ height: `${max > 0 ? Math.round((v / max) * 100) : 2}%` }} />
      ))}
    </div>
  );
}

export function InsightStrip(props: InsightStripProps) {
  const t = useT();
  const cards = buildInsightCards(
    {
      usage: props.usage,
      verificationRuns: props.verificationRuns,
      signals: props.signals,
      baseline: props.baseline,
      turns: props.turns,
    },
    t,
  );
  const [openId, setOpenId] = useState<InsightCardId | null>(null);

  return (
    <section className={styles.strip} aria-label={t('insight.stripAria')}>
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
                {/* PR-3 §3a — baselineDelta replaced by card.baseline (chip +
                    position + n + lowSample); render wiring lands in Task 4. */}
              </div>
              {card.sparkline && card.sparkline.length > 0 && (
                <Sparkline values={card.sparkline} tint={card.sparklineTint} />
              )}
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
