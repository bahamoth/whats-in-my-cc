import { describe, expect, it } from 'vitest';
import { buildEntityFacets } from '../entityFacets';
import type { GraphNodeDto } from '../../../../api/types';

const node = (id: string, kind: string, payload: unknown): GraphNodeDto => ({
  node_id: id,
  schema_version: 'graph_node.v1',
  session_id: 's',
  node_kind: kind,
  started_at: '2026-05-31T00:00:00Z',
  ended_at: null,
  merge_keys: {},
  source_event_ids: [id],
  source_uris: [],
  payload,
});

describe('buildEntityFacets (payload.facets)', () => {
  it('groups folded facets from owner node payload', () => {
    const nodes: GraphNodeDto[] = [
      node('call-1', 'tool_call', {
        facets: [
          { facet_kind: 'tool_result_log', basis: 'tool_use_id', source_event_id: 'e1', data: {} },
          { facet_kind: 'tool_decision_log', basis: 'tool_use_id', source_event_id: 'e2', data: {} },
        ],
      }),
      node('asst-1', 'assistant_message', {
        facets: [
          { facet_kind: 'llm_request_span', basis: 'request_id', source_event_id: 'e3', data: {} },
        ],
      }),
      node('plain-1', 'user_message', {}),
    ];
    const groups = buildEntityFacets(nodes);
    expect(groups.get('call-1')?.facets.length).toBe(2);
    expect(groups.get('asst-1')?.facets.length).toBe(1);
    expect(groups.has('plain-1')).toBe(false);
  });

  it('counts facets by kind', () => {
    const nodes: GraphNodeDto[] = [
      node('call-1', 'tool_call', {
        facets: [
          { facet_kind: 'tool_result_log', basis: 'tool_use_id', source_event_id: 'e1', data: {} },
          { facet_kind: 'tool_decision_log', basis: 'tool_use_id', source_event_id: 'e2', data: {} },
        ],
      }),
    ];
    const g = buildEntityFacets(nodes).get('call-1');
    expect(g?.byKind.tool_result_log).toBe(1);
    expect(g?.byKind.tool_decision_log).toBe(1);
  });

  it('ignores nodes without a facets array', () => {
    const nodes: GraphNodeDto[] = [
      node('a', 'tool_call', { facets: 'not-an-array' }),
      node('b', 'tool_call', null),
    ];
    expect(buildEntityFacets(nodes).size).toBe(0);
  });
});
