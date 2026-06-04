// Source-split Raw blocks, built DIRECTLY from ObservedEvents by correlation
// key — no graph node, no facet fold. The "a failed tool's error output must be
// visible in Raw" contract lives in buildRawBlocksFromEvents below.

import { asRecord } from '../../../lib/asRecord';
import type { RawBlock } from './RawTab';
import type { ObservedEventDto } from '../../../api/types';
import { parseLlmRequestSpan } from '../stream/llmRequestMetrics';

const TRANSCRIPT_NODE_KINDS = new Set(['tool_call', 'assistant_message', 'user_message']);
const TELEMETRY_LOG_NAMES = new Set(['tool_result', 'tool_decision', 'api_request']);

// Event-first Raw blocks: split a selected ObservedEvent into its own source +
// the correlated sources found among the loaded events (matched tool_result,
// folded-equivalent telemetry), WITHOUT a graph node or facet fold. Returns
// undefined when there is nothing extra to split → single-record fallback.
export function buildRawBlocksFromEvents(
  event: ObservedEventDto,
  events: ObservedEventDto[],
): RawBlock[] | undefined {
  const p = asRecord(event.payload);
  const entitySource = TRANSCRIPT_NODE_KINDS.has(event.kind) ? 'transcript' : event.kind;
  const entityLabel =
    event.kind === 'tool_call' && typeof p.tool_name === 'string' ? p.tool_name : event.kind;
  const entityBlock: RawBlock = { source: entitySource, label: entityLabel, record: event.payload };

  const extra: RawBlock[] = [];

  // tool_call output → its matched tool_result event (by tool_use_id), split so
  // a failed tool's error content is visible.
  if (event.kind === 'tool_call' && event.tool_use_id) {
    const resultEv = events.find(
      (e) => e.kind === 'tool_result' && e.tool_use_id === event.tool_use_id,
    );
    if (resultEv) {
      const tr = asRecord(resultEv.payload).tool_result;
      const hasTr =
        tr != null && typeof tr === 'object' && Object.keys(asRecord(tr)).length > 0;
      extra.push({
        source: 'tool_result',
        label: asRecord(tr).is_error === true ? 'error' : 'ok',
        record: hasTr ? tr : resultEv.payload,
      });
    }
  }

  // Correlated telemetry (the data the graph used to fold as facets), found here
  // by correlation key: log_records by tool_use_id / request_id, llm spans by
  // request_id. Labelled by event_name / span name.
  for (const e of events) {
    if (e.kind === 'log_record') {
      const lp = asRecord(e.payload);
      const name = lp.event_name;
      if (typeof name !== 'string' || !TELEMETRY_LOG_NAMES.has(name)) continue;
      const a = asRecord(lp.attributes);
      const match =
        (event.tool_use_id != null && a.tool_use_id === event.tool_use_id) ||
        (event.request_id != null && a.request_id === event.request_id);
      if (!match) continue;
      extra.push({ source: `${name}_log`, label: name, record: e.payload });
    } else if (e.kind === 'otel_span' && event.request_id != null) {
      const m = parseLlmRequestSpan(e.payload);
      if (!m || m.requestId !== event.request_id) continue;
      extra.push({ source: 'llm_request_span', label: 'claude_code.llm_request', record: e.payload });
    }
  }

  if (extra.length === 0) return undefined;
  return [entityBlock, ...extra];
}
