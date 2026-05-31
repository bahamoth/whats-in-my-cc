// Source-split Raw blocks for a selected graph node, extracted as a pure
// function so the "a failed tool's error output must be visible in Raw"
// contract is unit-testable.
//
// A tool_call node carries BOTH the call (input) and the tool_result (output,
// incl. is_error + the error content) — the latter merged in by the graph
// builder under payload.result.tool_result. The card / Insight derive the error
// badge from that, but the Raw tab previously fell back to the bare transcript
// line of the call event, which has no result — so the error output was
// invisible in Raw. We now split it into its own labelled block.

import { asRecord } from '../facets/entityFacets';
import type { RawBlock } from './RawTab';

const TRANSCRIPT_NODE_KINDS = new Set(['tool_call', 'assistant_message', 'user_message']);

interface NodeLike {
  node_id: string;
  node_kind: string;
  payload: unknown;
}
interface FacetLike {
  facet_kind: string;
  data: unknown;
}

export function buildRawBlocks(node: NodeLike, facets: FacetLike[]): RawBlock[] | undefined {
  const entitySource = TRANSCRIPT_NODE_KINDS.has(node.node_kind) ? 'transcript' : node.node_kind;
  const p = asRecord(node.payload);
  const entityLabel =
    node.node_kind === 'tool_call' && typeof p.tool_name === 'string' ? p.tool_name : node.node_kind;

  // tool_result lives at payload.result.tool_result (merged by the graph
  // builder). Split it out so a failed tool's error output is visible.
  const toolResult = asRecord(p.result).tool_result;
  const hasToolResult =
    toolResult != null && typeof toolResult === 'object' && Object.keys(asRecord(toolResult)).length > 0;

  // entity (call) block: the whole payload, minus the result when it is split
  // out below so it is not shown twice.
  const entityRecord = hasToolResult
    ? Object.fromEntries(Object.entries(p).filter(([k]) => k !== 'result'))
    : node.payload;
  const entityBlock: RawBlock = { source: entitySource, label: entityLabel, record: entityRecord };

  const toolResultBlock: RawBlock | null = hasToolResult
    ? {
        source: 'tool_result',
        label: asRecord(toolResult).is_error === true ? 'error' : 'ok',
        record: toolResult,
      }
    : null;

  // Folded telemetry facet blocks; label by event_name (logs) or raw_span.name
  // (span), falling back to the facet_kind.
  const facetBlocks: RawBlock[] = facets.map((f) => {
    const fd = asRecord(f.data);
    const rawSpan = asRecord(fd.raw_span);
    const label =
      typeof fd.event_name === 'string'
        ? fd.event_name
        : typeof rawSpan.name === 'string'
          ? rawSpan.name
          : f.facet_kind;
    return { source: f.facet_kind, label, record: f.data };
  });

  // Nothing extra to split (plain message, no facets, no tool_result) → let the
  // caller fall back to the bare single-record JsonTree (verbatim transcript),
  // keeping DetailPanel's "raw loaded" accent dot meaningful.
  const extra = [...(toolResultBlock ? [toolResultBlock] : []), ...facetBlocks];
  if (extra.length === 0) return undefined;
  return [entityBlock, ...extra];
}
