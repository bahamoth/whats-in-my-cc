// webui/src/components/replay/detail/InsightTab.tsx
//
// Metrics-led Insight tab (UX option A): for the selected event, show a
// compact header + the entity's COLLECTED metrics (EntityMetricsPanel, with
// plain-language ⓘ tooltips) + that event's Signals. The full raw payload
// lives in the Raw tab.
import { useState } from 'react';
import type { SignalDto, ObservedEventDto, EvidenceRef, LlmRequestP50Dto } from '../../../api/types';
import type { LlmRequestMetrics } from '../stream/llmRequestMetrics';
import { nodeLabel } from '../stream/nodeLabel';
import type { ToolMetrics } from './toolMetrics';
import { EntityMetricsPanel } from './EntityMetricsPanel';
import { eventProvenance } from './eventProvenance';
import { WhatSection } from './WhatSection';
import { McpPluginCard } from './McpPluginCard';
import { useT } from '../../../i18n';
import styles from './InsightTab.module.css';

interface InsightTabProps {
  signals: SignalDto[];
  event: ObservedEventDto | null;
  toolMetrics: ToolMetrics | null;
  llmMetrics: LlmRequestMetrics | null;
  /** Session-wide p50 baselines for the request-metric rows (PR-3 §3d). */
  llmP50?: LlmRequestP50Dto | null;
  /** The matching tool_result event when the selected event is a tool_call. */
  matchedResult?: ObservedEventDto | null;
  /** S7 — jump the stream + detail to an evidence event id (clicking a Signal).
   *  Reuses the page's selection mechanism (loadAround handles out-of-window). */
  onSelectEvent?: (eventId: string) => void;
}

/** S7 — resolve an evidence ref to a jumpable event id (refs are either a bare
 *  ULID string or an `{ kind, event_id }` object). */
function evidenceEventId(ref: EvidenceRef): string | null {
  if (typeof ref === 'string') return ref;
  return typeof ref.event_id === 'string' ? ref.event_id : null;
}

const KIND_ICON: Record<string, string> = {
  tool: '⚙',
  assistant: '✦',
  thinking: '…',
  user: '◎',
  hook: '↩',
  span: '◇',
  verify: '✓',
  diff: '±',
  other: '·',
};

/** A correlation-id chip. The value is visually clipped (long ids), but clicking
 *  copies the FULL id — an observability tool must never hide the identifier a
 *  user needs to grep. Full value also lives in the title (hover reveals it). */
function CopyChip({ field, label, value }: { field: string; label: string; value: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className={styles.corrChip}
      title={t('detail.insightTab.copyTitle', { field, value })}
      aria-label={`${field} ${value}`}
      onClick={() => {
        navigator.clipboard?.writeText(value);
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      }}
    >
      {label} <code>{value}</code>
      <span className={styles.copyHint} aria-hidden>{copied ? '✓' : '⧉'}</span>
    </button>
  );
}

function SignalsList({
  signals,
  onSelectEvent,
}: {
  signals: SignalDto[];
  onSelectEvent?: (eventId: string) => void;
}) {
  const t = useT();
  return (
    <ul className={styles.list}>
      {signals.map((s) => {
        const target = s.evidence_refs.map(evidenceEventId).find((id): id is string => !!id);
        const jumpable = Boolean(target && onSelectEvent);
        return (
          <li
            key={s.signal_id}
            className={styles.item}
            data-jumpable={jumpable ? 'true' : undefined}
            role={jumpable ? 'button' : undefined}
            tabIndex={jumpable ? 0 : undefined}
            title={jumpable ? t('detail.insightTab.jumpToEvidence') : undefined}
            onClick={jumpable ? () => onSelectEvent!(target!) : undefined}
            onKeyDown={
              jumpable
                ? (e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelectEvent!(target!);
                    }
                  }
                : undefined
            }
          >
            <div className={styles.head}>
              <span className={styles.detector}>{s.detector}</span>
              {s.subkind && <span className={styles.subkind}>{s.subkind}</span>}
              {jumpable && <span className={styles.jumpHint} aria-hidden>{t('detail.insightTab.evidenceHint')}</span>}
            </div>
            <p className={styles.summary}>{s.summary}</p>
          </li>
        );
      })}
    </ul>
  );
}

export function InsightTab({
  signals,
  event,
  toolMetrics,
  llmMetrics,
  llmP50 = null,
  matchedResult,
  onSelectEvent,
}: InsightTabProps) {
  const t = useT();
  if (!event && signals.length === 0) {
    return (
      <div className={styles.root}>
        <p className={styles.empty}>No insights for this event.</p>
      </div>
    );
  }

  const label = event ? nodeLabel({ node_kind: event.kind, payload: event.payload, telemetry: event.telemetry, tag: event.tag, is_meta: event.is_meta }, t) : null;
  const icon = label ? KIND_ICON[label.kind] ?? KIND_ICON.other : null;
  const prov = event ? eventProvenance(event.kind) : null;

  return (
    <div className={styles.root}>
      {event && (
        <>
          {/* H: header + provenance badge + correlation chips */}
          <div className={styles.nodeHeader}>
            <span className={styles.nodeIcon} aria-hidden="true">{icon}</span>
            <span className={styles.nodePrimary}>{label?.primary}</span>
            {prov && (
              <span
                className={prov.kind === 'native' ? styles.badgeNative : styles.badgeDerived}
                title={prov.kind === 'native' ? t('detail.insightTab.nativeTitle') : t('detail.insightTab.derivedTitle')}
              >
                {t(prov.kind === 'native' ? 'detail.provenance.native' : 'detail.provenance.derived')}
              </span>
            )}
            <span className={styles.nodeId}>{event.event_id}</span>
          </div>

          {/* Correlation chips (tool_use_id / request_id / turn_id) — click to copy
              the FULL id (the value is only visually clipped). */}
          {(event.tool_use_id || event.request_id || event.turn_id) && (
            <div className={styles.corrChips}>
              {event.tool_use_id && <CopyChip field="tool_use_id" label="tool" value={event.tool_use_id} />}
              {event.request_id && <CopyChip field="request_id" label="req" value={event.request_id} />}
              {event.turn_id && <CopyChip field="turn_id" label="turn" value={event.turn_id} />}
            </div>
          )}

          {/* ① WHAT: what this event did */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>{t('detail.insightTab.whatTitle')}</div>
            <WhatSection event={event} matchedResult={matchedResult ?? null} />
          </div>

          {/* ② HOW: execution metrics */}
          <div className={styles.section}>
            <div className={styles.sectionTitle}>{t('detail.insightTab.howTitle')}</div>
            <EntityMetricsPanel
              kind={event.kind}
              toolMetrics={toolMetrics}
              llmMetrics={llmMetrics}
              payload={event.payload}
              llmP50={llmP50}
            />
          </div>

          {/* Plugin reference — only for MCP tool calls (server·tool). */}
          {event.kind === 'tool_call' && event.tool_name?.startsWith('mcp__') && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>{t('detail.plugin.title')}</div>
              <McpPluginCard toolName={event.tool_name} />
            </div>
          )}
        </>
      )}

      {/* ③ SIGNALS */}
      {signals.length > 0 && (
        <div className={styles.signalsSection}>
          <div className={styles.sectionTitle}>Signals</div>
          <SignalsList signals={signals} onSelectEvent={onSelectEvent} />
        </div>
      )}
    </div>
  );
}
