import type { GraphNodeDto } from '../../../api/types';

/** A folded telemetry facet carried on an owner node's `payload.facets`.
 *  `data` is the folded telemetry event's verbatim payload:
 *    - tool_result_log / tool_decision_log / api_request_log → `data.attributes`
 *      is a flat map (string-valued metrics).
 *    - llm_request_span → `data.raw_span.attributes` is an OTLP `{key,value}[]`.
 *  See src/graph/build.rs (telemetry fold) for the producer. */
export interface FacetEntry {
  facet_kind: string;
  basis: string;
  source_event_id: string;
  data: Record<string, unknown>;
}

export interface FacetGroup {
  entityNodeId: string;
  facets: FacetEntry[];
  byKind: Record<string, number>;
}

/** Narrow an `unknown` to a plain object by a runtime check, returning an empty
 *  object for null/non-object inputs. Lets callers read fields off untrusted
 *  payloads/facet `data` without throwing or blind `as` casts. */
export function asRecord(v: unknown): Record<string, unknown> {
  return v != null && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

/** Group folded telemetry facets by owner node. Reads `node.payload.facets`
 *  (set by the backend telemetry-fold pass); `facet_of` edges no longer exist.
 *  Owner nodes without a non-empty `facets` array are omitted. */
export function buildEntityFacets(nodes: GraphNodeDto[]): Map<string, FacetGroup> {
  const out = new Map<string, FacetGroup>();
  for (const n of nodes) {
    const p = asRecord(n.payload);
    const facets = Array.isArray(p.facets) ? (p.facets as FacetEntry[]) : [];
    if (facets.length === 0) continue;
    const byKind: Record<string, number> = {};
    for (const f of facets) byKind[f.facet_kind] = (byKind[f.facet_kind] ?? 0) + 1;
    out.set(n.node_id, { entityNodeId: n.node_id, facets, byKind });
  }
  return out;
}
