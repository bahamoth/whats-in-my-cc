// webui/src/components/replay/detail/toolMetrics.ts
//
// Folded tool-execution facets (FacetEntry[]) → one ToolMetrics struct.
// The tool_result_log / tool_decision_log facets carry their telemetry under
// `facet.data.attributes` (a flat map). `facet_of` edges no longer exist; the
// facets are read from the owner node's payload via buildEntityFacets.
//
// Real-data shape (tests/fixtures/facet/real/facet_correlation_v01.json):
//   data: { event_name: "tool_result" | "tool_decision", attributes: { … } }
//   attributes는 flat object; 값은 대부분 string ("57", "true") 이지만
//   event.sequence 등 number인 경우도 있다.
import { asRecord, type FacetEntry } from '../facets/entityFacets';

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

/** Fold tool_result_log + tool_decision_log facets (data.attributes) into one
 *  ToolMetrics. Non-tool facet kinds are ignored. */
export function buildToolMetrics(facets: FacetEntry[]): ToolMetrics {
  const m: ToolMetrics = {
    durationMs: null,
    success: null,
    decisionSource: null,
    decisionType: null,
    inputBytes: null,
    resultBytes: null,
    sequence: null,
  };
  for (const f of facets) {
    if (f.facet_kind !== 'tool_result_log' && f.facet_kind !== 'tool_decision_log') continue;
    const a = asRecord(asRecord(f.data).attributes);
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
