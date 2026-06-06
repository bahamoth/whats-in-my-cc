// Detail-panel metrics derived DIRECTLY from ObservedEvents by correlation key
// (tool_use_id / request_id) — NO graph node, NO facet fold. This is the
// event-first replacement for the graph-facet path: the same telemetry the
// graph builder used to fold onto an owner node is found here by querying the
// loaded event window, so the detail view no longer depends on the graph.
//
// Real payload shapes (DB-verified):
//   tool_result / tool_decision log_record:
//     payload = { event_name, attributes: { tool_use_id, success, duration_ms, … } }
//   llm_request span:  telemetry facet = { span_name, attributes: { … flat } }
//     (C4 Tier 3-1: read from the facet, not the removed payload.raw_span)
//   api_request log:   payload = { event_name:'api_request', attributes: { request_id, cost_usd, … } }
import type { ObservedEventDto } from '../../../api/types';
import { asRecord as asObj } from '../../../lib/asRecord';
import { buildToolMetrics, type ToolMetrics } from './toolMetrics';
import {
  parseLlmRequestSpan,
  parseApiRequestLog,
  type LlmRequestMetrics,
} from '../stream/llmRequestMetrics';

/** Tool-execution metrics for a tool_call, found by tool_use_id among the loaded
 *  events. A tool_result / tool_decision log_record's payload IS the same shape
 *  the fold used as `facet.data`, so we wrap each match as a pseudo-facet and
 *  reuse the existing buildToolMetrics parser verbatim. */
export function buildToolMetricsFromEvents(
  events: ObservedEventDto[],
  toolUseId: string | null,
): ToolMetrics {
  if (!toolUseId) return buildToolMetrics([]);
  // collect the `attributes` maps of the tool_result / tool_decision
  // log_records that share this tool_use_id, then parse them into ToolMetrics.
  const attrsList: Record<string, unknown>[] = [];
  for (const e of events) {
    if (e.kind !== 'log_record') continue;
    const p = asObj(e.payload);
    const name = p.event_name;
    if (name !== 'tool_result' && name !== 'tool_decision') continue;
    const attrs = asObj(p.attributes);
    if (attrs.tool_use_id !== toolUseId) continue;
    attrsList.push(attrs);
  }
  // Transcript fallback for `success`. A tool call rejected at the input-
  // validation stage (e.g. Edit "File has not been read yet") emits NO OTel
  // tool_result / tool_decision log_record, but the transcript tool_result event
  // (kind 'tool_result') still records is_error — exactly the failures most worth
  // surfacing. Without this the detail panel falsely reads "지표 미수집". Folded
  // LAST so OTel telemetry (authoritative, string "true"/"false") wins on
  // first-non-null. DB-verified shape: payload.tool_result.is_error (true only on
  // error; absent on success). Other ToolMetrics fields are OTel-only and stay
  // honestly null. tool_result is the only kind here, so any leaked log_record
  // tool_result (already folded above) can't double-count.
  const transcript = events.find(
    (e) => e.kind === 'tool_result' && e.tool_use_id === toolUseId,
  );
  if (transcript) {
    const tr = asObj(asObj(transcript.payload).tool_result);
    attrsList.push({ success: tr.is_error === true ? 'false' : 'true' });
  }
  return buildToolMetrics(attrsList);
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
    const m = parseLlmRequestSpan(e);
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
