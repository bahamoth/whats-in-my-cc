// webui/src/components/replay/stream/llmRequestMetrics.ts
//
// Per-response (LLM request) metrics, joined to transcript events by
// `request_id`. A `thinking` (or assistant_message) event and its
// `claude_code.llm_request` OTel span share the same request_id — the span
// carries the only honest "how long / how much" signals (the thinking
// plaintext is recorded nowhere). We read those metrics from the span's
// telemetry facet that lives in the windowed events.
//
// C4 (Tier 3-1): the span used to be re-embedded under `payload.raw_span` (an
// OTLP `attributes: [{key,value:{stringValue|intValue}}]` array). That
// double-store was removed; span name + attributes now come from the telemetry
// facet, whose `attributes` is a FLAT key→value object (backend `flatten_kv`).
import type { ObservedEventDto, TelemetryFacetDto } from '../../../api/types';
import type { TFunction } from '../../../i18n';

export interface LlmRequestMetrics {
  requestId: string;
  /** total request wall-clock (ms) */
  durationMs: number | null;
  /** time-to-first-token (ms) */
  ttftMs: number | null;
  inputTokens: number | null;
  /** generated tokens (includes thinking) */
  outputTokens: number | null;
  cacheReadTokens: number | null;
  cacheCreationTokens: number | null;
  /** tool_use | end_turn | max_tokens | … */
  stopReason: string | null;
  /** request attempt count; >1 means it was retried */
  attempt: number | null;
  success: boolean | null;
  model: string | null;
  /** measured request cost in USD (Claude Code's own figure, from the
   *  api_request_log facet — NOT the token×public-rate estimate). null when
   *  the log facet is absent. */
  costUsd: number | null;
  /** what issued the request — `repl_main_thread` (the main conversation) or
   *  `agent:builtin:<name>` / `agent:custom` (a subagent). From the
   *  api_request_log facet; optional so span-only callers need not set it. */
  querySource?: string | null;
}

function num(x: unknown): number | null {
  const n = typeof x === 'number' ? x : typeof x === 'string' ? Number(x) : NaN;
  return Number.isFinite(n) ? n : null;
}

function bool(x: unknown): boolean | null {
  if (typeof x === 'boolean') return x;
  if (x === 'true') return true;
  if (x === 'false') return false;
  return null;
}

function str(x: unknown): string | null {
  return typeof x === 'string' && x.length > 0 ? x : null;
}

/** The shape parseLlmRequestSpan reads: an otel_span event carrying its
 *  telemetry facet. We accept the whole event (or anything with a `telemetry`
 *  field) so callers pass the event, not its (now-empty) payload. */
type SpanFacetCarrier = { telemetry?: TelemetryFacetDto | null } | null | undefined;

/** Parse a `claude_code.llm_request` span's telemetry facet into
 *  LlmRequestMetrics. Reads the facet's `span_name` + FLAT `attributes`
 *  (backend `flatten_kv` already unwrapped the OTLP array). Returns null if the
 *  event is not a `claude_code.llm_request` span or has no resolvable
 *  request_id. C4 (Tier 3-1): replaces the removed `payload.raw_span` read. */
export function parseLlmRequestSpan(event: unknown): LlmRequestMetrics | null {
  const facet = (event as SpanFacetCarrier)?.telemetry;
  if (!facet || facet.span_name !== 'claude_code.llm_request') return null;

  const attrs = facet.attributes ?? {};
  const rid = str(attrs['request_id']) ?? str(attrs['gen_ai.response.id']);
  if (!rid) return null;

  return {
    requestId: rid,
    durationMs: num(attrs['duration_ms']),
    ttftMs: num(attrs['ttft_ms']),
    inputTokens: num(attrs['input_tokens']),
    outputTokens: num(attrs['output_tokens']),
    cacheReadTokens: num(attrs['cache_read_tokens']),
    cacheCreationTokens: num(attrs['cache_creation_tokens']),
    stopReason: str(attrs['stop_reason']),
    attempt: num(attrs['attempt']),
    success: bool(attrs['success']),
    model: str(attrs['model']),
    // cost lives in the api_request_log facet, not the span — null here, merged
    // in by the caller via parseApiRequestLog.
    costUsd: num(attrs['cost_usd']),
  };
}

/** Parse the `api_request` log_record's payload. Its `attributes` is a plain
 *  key→value record, and it carries Claude Code's own measured per-request
 *  `cost_usd` (the authoritative cost, distinct from the WebUI's
 *  token×public-rate estimate). Returns null when the payload is not an
 *  api_request log with an attributes record. */
export function parseApiRequestLog(
  payload: unknown,
): { costUsd: number | null; querySource: string | null } | null {
  const attrs = (payload as { attributes?: Record<string, unknown> } | null)?.attributes;
  if (!attrs || typeof attrs !== 'object') return null;
  return { costUsd: num(attrs['cost_usd']), querySource: str(attrs['query_source']) };
}

/** Build a `request_id → LlmRequestMetrics` map from `claude_code.llm_request`
 *  spans present in the given event window. Events without the span simply
 *  have no entry (the marker then omits the metrics, degrading gracefully). */
export function buildLlmRequestMetrics(events: ObservedEventDto[]): Map<string, LlmRequestMetrics> {
  const map = new Map<string, LlmRequestMetrics>();
  for (const e of events) {
    if (e.kind !== 'otel_span') continue;
    const m = parseLlmRequestSpan(e);
    if (m) map.set(m.requestId, m);
  }
  return map;
}

export function formatDuration(ms: number | null): string | null {
  if (ms == null) return null;
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
}

export function formatTokens(n: number | null): string | null {
  if (n == null) return null;
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
  return `${n}`;
}

/** Format a USD cost: 4 decimals under $1 (per-request costs are often
 *  sub-cent), 2 at/above. */
export function formatUsd(n: number | null): string | null {
  if (n == null) return null;
  return `$${n.toFixed(n < 1 ? 4 : 2)}`;
}

/** Human label for the request's query_source (who issued it). l10n — the
 *  caller injects t() for the localized "main thread" / "subagent" wording. */
export function formatQuerySource(qs: string | null | undefined, t: TFunction): string | null {
  if (!qs) return null;
  if (qs === 'repl_main_thread') return t('stream.querySource.mainThread');
  if (qs.startsWith('agent:builtin:')) return t('stream.querySource.subagent', qs.slice('agent:builtin:'.length));
  if (qs.startsWith('agent:')) return t('stream.querySource.subagent', qs.slice('agent:'.length));
  return qs;
}

/** Output generation throughput (tokens/sec) from output tokens + wall-clock,
 *  or null when either is missing/zero. */
export function formatThroughput(outputTokens: number | null, durationMs: number | null): string | null {
  if (outputTokens == null || durationMs == null || durationMs <= 0) return null;
  return `${Math.round(outputTokens / (durationMs / 1000))} tok/s`;
}
