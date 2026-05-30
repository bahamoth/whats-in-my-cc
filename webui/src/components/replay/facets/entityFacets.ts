import type { GraphNodeDto, GraphEdgeDto } from '../../../api/types';

export interface FacetGroup {
  entityNodeId: string;
  facetNodeIds: string[];
  byKind: Record<string, number>;
}

/** facet_of 엣지(from=facet, to=엔티티)를 따라 엔티티별 facet 묶음을 만든다. */
export function buildEntityFacets(
  nodes: GraphNodeDto[],
  edges: GraphEdgeDto[],
): Map<string, FacetGroup> {
  const kindById = new Map(nodes.map((n) => [n.node_id, n.node_kind]));
  const out = new Map<string, FacetGroup>();
  for (const e of edges) {
    if (e.edge_kind !== 'facet_of') continue;
    const entity = e.to_node_id;
    const facet = e.from_node_id;
    let g = out.get(entity);
    if (!g) { g = { entityNodeId: entity, facetNodeIds: [], byKind: {} }; out.set(entity, g); }
    g.facetNodeIds.push(facet);
    const k = kindById.get(facet) ?? 'unknown';
    g.byKind[k] = (g.byKind[k] ?? 0) + 1;
  }
  return out;
}
