// webui/src/components/replay/detail/toolMetrics.ts
//
// log_record facet 노드들(payload.attributes)에서 도구 실행 지표를 추출한다.
// `facet_of` 엣지로 연결된 tool_call 노드의 log_record 패싯들을 받아
// ToolMetrics 구조체 하나로 합친다.
//
// Real-data shape (tests/fixtures/facet/real/facet_correlation_v01.json):
//   payload: { event_name: "tool_result" | "tool_decision", attributes: { … } }
//   attributes는 flat object; 값은 대부분 string ("57", "true") 이지만
//   event.sequence 등 number인 경우도 있다.
import type { GraphNodeDto } from '../../../api/types';

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

/** log_record facet 노드들(payload.attributes)에서 도구 실행 지표를 합친다. */
export function buildToolMetrics(facetNodes: GraphNodeDto[]): ToolMetrics {
  const m: ToolMetrics = {
    durationMs: null, success: null, decisionSource: null, decisionType: null,
    inputBytes: null, resultBytes: null, sequence: null,
  };
  for (const n of facetNodes) {
    if (n.node_kind !== 'log_record') continue;
    const p = (n.payload ?? {}) as Record<string, unknown>;
    const a = (p.attributes ?? {}) as Record<string, unknown>;
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
