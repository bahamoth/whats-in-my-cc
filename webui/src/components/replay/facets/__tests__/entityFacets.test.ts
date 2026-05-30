import { describe, expect, it } from 'vitest';
import { buildEntityFacets } from '../entityFacets';
import type { GraphNodeDto, GraphEdgeDto } from '../../../../api/types';

const node = (id: string, kind: string): GraphNodeDto => ({
  node_id: id, schema_version: '1', session_id: 's', node_kind: kind,
  started_at: '', ended_at: null, merge_keys: {}, source_event_ids: [id + '-ev'],
  source_uris: [], payload: {},
});
const facetEdge = (from: string, to: string, basis: string): GraphEdgeDto => ({
  edge_id: `${from}->${to}`, schema_version: '1', session_id: 's',
  from_node_id: from, to_node_id: to, edge_kind: 'facet_of',
  origin: 'deterministic', attributes: { basis },
});

describe('buildEntityFacets', () => {
  it('maps an entity to its facet node ids via facet_of edges', () => {
    const nodes = [node('call', 'tool_call'), node('log', 'log_record')];
    const edges = [facetEdge('log', 'call', 'tool_use_id')];
    const m = buildEntityFacets(nodes, edges);
    expect(m.get('call')?.facetNodeIds).toEqual(['log']);
    expect(m.get('call')?.byKind['log_record']).toBe(1);
  });
  it('groups multiple facets under one entity', () => {
    const nodes = [node('call', 'tool_call'), node('l1', 'log_record'), node('l2', 'log_record')];
    const edges = [facetEdge('l1', 'call', 'tool_use_id'), facetEdge('l2', 'call', 'tool_use_id')];
    const m = buildEntityFacets(nodes, edges);
    expect(m.get('call')?.facetNodeIds.sort()).toEqual(['l1', 'l2']);
    expect(m.get('call')?.byKind['log_record']).toBe(2);
  });
  it('ignores non-facet_of edges', () => {
    const nodes = [node('a', 'tool_call'), node('b', 'tool_result')];
    const edges: GraphEdgeDto[] = [{ edge_id: 'x', schema_version: '1', session_id: 's',
      from_node_id: 'a', to_node_id: 'b', edge_kind: 'tool_call_to_result', origin: 'deterministic', attributes: {} }];
    expect(buildEntityFacets(nodes, edges).size).toBe(0);
  });
});
