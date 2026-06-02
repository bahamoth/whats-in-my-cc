// Detail-panel metrics derived DIRECTLY from ObservedEvents by correlation key
// (tool_use_id / request_id) — NO graph node, NO facet fold. This is the
// event-first replacement for the graph-facet path: the same telemetry the
// graph builder used to fold onto an owner node is found here by querying the
// loaded event window, so the detail view no longer depends on the graph.
//
// Real payload shapes (DB-verified):
//   tool_result / tool_decision log_record:
//     payload = { event_name, attributes: { tool_use_id, success, duration_ms, … } }
//   llm_request span:  payload = { raw_span: { name, attributes: [{key,value}] } }
//   api_request log:   payload = { event_name:'api_request', attributes: { request_id, cost_usd, … } }
import type { ObservedEventDto } from '../../../api/types';
import type { FacetEntry } from '../facets/entityFacets';
import { buildToolMetrics, type ToolMetrics } from './toolMetrics';
import {
  parseLlmRequestSpan,
  parseApiRequestLog,
  type LlmRequestMetrics,
} from '../stream/llmRequestMetrics';

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

/** Tool-execution metrics for a tool_call, found by tool_use_id among the loaded
 *  events. A tool_result / tool_decision log_record's payload IS the same shape
 *  the fold used as `facet.data`, so we wrap each match as a pseudo-facet and
 *  reuse the existing buildToolMetrics parser verbatim. */
export function buildToolMetricsFromEvents(
  events: ObservedEventDto[],
  toolUseId: string | null,
): ToolMetrics {
  if (!toolUseId) return buildToolMetrics([]);
  const facets: FacetEntry[] = [];
  for (const e of events) {
    if (e.kind !== 'log_record') continue;
    const p = asObj(e.payload);
    const name = p.event_name;
    if (name !== 'tool_result' && name !== 'tool_decision') continue;
    if (asObj(p.attributes).tool_use_id !== toolUseId) continue;
    facets.push({
      facet_kind: name === 'tool_result' ? 'tool_result_log' : 'tool_decision_log',
      basis: 'tool_use_id',
      source_event_id: e.event_id,
      data: p,
    });
  }
  return buildToolMetrics(facets);
}

/** Per-response LLM metrics for an assistant turn, found by request_id: the
 *  llm_request span (base metrics) merged with the api_request log's measured
 *  cost. Mirrors the old graph-facet merge, sourced from events. */
export function buildLlmMetricsFromEvents(
  events: ObservedEventDto[],
  requestId: string | null,
): LlmRequestMetrics | null {
  if (!requestId) return null;
  let base: LlmRequestMetrics | null = null;
  for (const e of events) {
    if (e.kind !== 'otel_span') continue;
    const m = parseLlmRequestSpan(e.payload);
    if (m && m.requestId === requestId) {
      base = m;
      break;
    }
  }
  if (!base) return null;
  let extra: { costUsd: number | null; querySource: string | null } | null = null;
  for (const e of events) {
    if (e.kind !== 'log_record') continue;
    const p = asObj(e.payload);
    if (p.event_name !== 'api_request') continue;
    if (asObj(p.attributes).request_id !== requestId) continue;
    extra = parseApiRequestLog(p);
    break;
  }
  return { ...base, costUsd: extra?.costUsd ?? null, querySource: extra?.querySource ?? null };
}
