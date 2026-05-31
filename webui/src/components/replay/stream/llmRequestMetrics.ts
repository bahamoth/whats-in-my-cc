// webui/src/components/replay/stream/llmRequestMetrics.ts
//
// Per-response (LLM request) metrics, joined to transcript events by
// `request_id`. A `thinking` (or assistant_message) event and its
// `claude_code.llm_request` OTel span share the same request_id — the span
// carries the only honest "how long / how much" signals (the thinking
// plaintext is recorded nowhere). We read those metrics from the span
// attributes that live in the windowed events.
import type { ObservedEventDto } from '../../../api/types';

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

type OtlpAttrValue =
  | { stringValue?: string; intValue?: string | number; doubleValue?: number; boolValue?: boolean }
  | undefined;

function attrVal(v: OtlpAttrValue): string | number | boolean | null {
  if (!v || typeof v !== 'object') return null;
  if (typeof v.stringValue === 'string') return v.stringValue;
  if (v.intValue != null) return typeof v.intValue === 'string' ? Number(v.intValue) : v.intValue;
  if (typeof v.doubleValue === 'number') return v.doubleValue;
  if (typeof v.boolValue === 'boolean') return v.boolValue;
  return null;
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

type LlmRequestSpanPayload = {
  raw_span?: { name?: string; attributes?: Array<{ key: string; value: OtlpAttrValue }> };
} | null;

/** Parse a single `claude_code.llm_request` span payload into LlmRequestMetrics.
 *  Accepts either a windowed otel_span event payload or a folded
 *  `llm_request_span` facet's `data` — both carry the same
 *  `raw_span.attributes[]` OTLP shape. Returns null if the payload is not a
 *  `claude_code.llm_request` span or has no resolvable request_id. */
export function parseLlmRequestSpan(payload: unknown): LlmRequestMetrics | null {
  const span = (payload as LlmRequestSpanPayload)?.raw_span;
  if (!span || span.name !== 'claude_code.llm_request') return null;

  const attrs: Record<string, string | number | boolean | null> = {};
  for (const a of span.attributes ?? []) attrs[a.key] = attrVal(a.value);

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

/** Parse the folded `api_request_log` facet's `data`. Unlike the OTLP span,
 *  its `attributes` is a plain key→value record, and it carries Claude Code's
 *  own measured per-request `cost_usd` (the authoritative cost, distinct from
 *  the WebUI's token×public-rate estimate). Returns null when the payload is
 *  not an api_request_log with an attributes record. */
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
    const m = parseLlmRequestSpan(e.payload);
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

/** Human label for the request's query_source (who issued it). */
export function formatQuerySource(qs: string | null | undefined): string | null {
  if (!qs) return null;
  if (qs === 'repl_main_thread') return '메인 스레드';
  if (qs.startsWith('agent:builtin:')) return `서브에이전트 · ${qs.slice('agent:builtin:'.length)}`;
  if (qs.startsWith('agent:')) return `서브에이전트 · ${qs.slice('agent:'.length)}`;
  return qs;
}

/** Output generation throughput (tokens/sec) from output tokens + wall-clock,
 *  or null when either is missing/zero. */
export function formatThroughput(outputTokens: number | null, durationMs: number | null): string | null {
  if (outputTokens == null || durationMs == null || durationMs <= 0) return null;
  return `${Math.round(outputTokens / (durationMs / 1000))} tok/s`;
}
