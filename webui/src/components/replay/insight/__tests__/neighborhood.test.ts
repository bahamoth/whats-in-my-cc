// webui/src/components/replay/insight/__tests__/neighborhood.test.ts
/** R5 RED — neighborhood extracts a bounded subgraph around the center. Plan R5 Task 1. */
import { describe, expect, it } from 'vitest';
import { neighborhood } from '../neighborhood';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

function n(id: string): GraphNodeDto {
  return { node_id: id, schema_version: '1', session_id: 's', node_kind: 'k', started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload: {} };
}
function e(id: string, from: string, to: string): GraphEdgeDto {
  return { edge_id: id, schema_version: '1', session_id: 's', from_node_id: from, to_node_id: to, edge_kind: 'x', origin: 'deterministic', attributes: {}, inference_rule_id: null, confidence: null };
}
// chain a -> b -> c -> d ; plus b -> x
const nodes = ['a', 'b', 'c', 'd', 'x'].map(n);
const edges = [e('e1', 'a', 'b'), e('e2', 'b', 'c'), e('e3', 'c', 'd'), e('e4', 'b', 'x')];

describe('neighborhood', () => {
  it('returns empty for a null/absent center', () => {
    expect(neighborhood(nodes, edges, null, 1)).toEqual({ nodes: [], edges: [] });
    expect(neighborhood(nodes, edges, 'zzz', 1)).toEqual({ nodes: [], edges: [] });
  });

  it('1 hop around b includes a, b, c, x (upstream + downstream) but not d', () => {
    const sub = neighborhood(nodes, edges, 'b', 1);
    expect(new Set(sub.nodes.map((nn) => nn.node_id))).toEqual(new Set(['a', 'b', 'c', 'x']));
    expect(sub.nodes.find((nn) => nn.node_id === 'd')).toBeUndefined();
  });

  it('keeps only edges whose both endpoints are in the kept set', () => {
    const sub = neighborhood(nodes, edges, 'b', 1);
    const ids = new Set(sub.edges.map((ed) => ed.edge_id));
    expect(ids).toEqual(new Set(['e1', 'e2', 'e4'])); // not e3 (c->d, d excluded)
  });

  it('2 hops around b reaches d', () => {
    const sub = neighborhood(nodes, edges, 'b', 2);
    expect(sub.nodes.find((nn) => nn.node_id === 'd')).toBeDefined();
    expect(sub.edges.map((ed) => ed.edge_id)).toContain('e3');
  });

  it('lists the center node first', () => {
    const sub = neighborhood(nodes, edges, 'b', 1);
    expect(sub.nodes[0].node_id).toBe('b');
  });
});
