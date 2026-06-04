// webui/src/components/replay/detail/toolMetrics.ts
//
// Tool-execution metrics, parsed from the `attributes` maps of a tool_call's
// correlated tool_result / tool_decision log_record events (found by
// tool_use_id; see eventMetrics.ts). No graph node, no facet — just the flat
// telemetry attribute maps.
//
// Real-data shape (DB-verified tool_result log_record):
//   payload.attributes = { tool_use_id, success:"true", duration_ms:"57",
//     tool_input_size_bytes, tool_result_size_bytes, decision_source,
//     decision_type, "event.sequence", … }  (values mostly string)

export interface ToolMetrics {
  durationMs: number | null;
  success: boolean | null;
  decisionSource: string | null;
  decisionType: string | null;
  inputBytes: number | null;
  resultBytes: number | null;
  sequence: number | null;
}

function num(v: unknown): number | null {
  if (typeof v === 'number') return v;
  if (typeof v === 'string' && v.trim() !== '' && !Number.isNaN(Number(v))) return Number(v);
  return null;
}

function str(v: unknown): string | null {
  return typeof v === 'string' ? v : null;
}

/** Fold the `attributes` maps of the matching tool_result / tool_decision
 *  log_records into one ToolMetrics. First non-null wins per field. */
export function buildToolMetrics(attrsList: Record<string, unknown>[]): ToolMetrics {
  const m: ToolMetrics = {
    durationMs: null,
    success: null,
    decisionSource: null,
    decisionType: null,
    inputBytes: null,
    resultBytes: null,
    sequence: null,
  };
  for (const a of attrsList) {
    if (m.durationMs == null) m.durationMs = num(a.duration_ms);
    if (m.success == null && typeof a.success === 'string') m.success = a.success === 'true';
    if (m.inputBytes == null) m.inputBytes = num(a.tool_input_size_bytes);
    if (m.resultBytes == null) m.resultBytes = num(a.tool_result_size_bytes);
    if (m.decisionSource == null) m.decisionSource = str(a.decision_source);
    if (m.decisionType == null) m.decisionType = str(a.decision_type);
    if (m.sequence == null) m.sequence = num(a['event.sequence']);
  }
  return m;
}
