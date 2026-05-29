import type { GraphNodeDto, GraphEdgeDto } from '../../../api/types';

export interface SubGraph { nodes: GraphNodeDto[]; edges: GraphEdgeDto[]; }

export function neighborhood(
  nodes: GraphNodeDto[],
  edges: GraphEdgeDto[],
  centerId: string | null,
  hops: number,
): SubGraph {
  if (!centerId || !nodes.some((n) => n.node_id === centerId)) {
    return { nodes: [], edges: [] };
  }
  // undirected adjacency
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (!adj.has(e.from_node_id)) adj.set(e.from_node_id, []);
    adj.get(e.from_node_id)!.push(e.to_node_id);
    if (!adj.has(e.to_node_id)) adj.set(e.to_node_id, []);
    adj.get(e.to_node_id)!.push(e.from_node_id);
  }
  // BFS to `hops`, center first
  const order: string[] = [centerId];
  const dist = new Map<string, number>([[centerId, 0]]);
  const queue = [centerId];
  while (queue.length) {
    const cur = queue.shift()!;
    const d = dist.get(cur)!;
    if (d >= hops) continue;
    for (const nb of adj.get(cur) ?? []) {
      if (!dist.has(nb)) {
        dist.set(nb, d + 1);
        order.push(nb);
        queue.push(nb);
      }
    }
  }
  const keep = new Set(order);
  const byId = new Map(nodes.map((n) => [n.node_id, n]));
  const subNodes = order.map((id) => byId.get(id)).filter((n): n is GraphNodeDto => !!n);
  const subEdges = edges.filter((e) => keep.has(e.from_node_id) && keep.has(e.to_node_id));
  return { nodes: subNodes, edges: subEdges };
}
