// webui/src/components/replay/stream/streamModel.ts
import type { ObservedEventDto } from '../../../api/types';
import type { LlmRequestMetrics } from './llmRequestMetrics';
import { messageOrigin, type MessageOrigin } from './messageOrigin';

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

// ---------------------------------------------------------------------------
// buildStreamModel — Slice S2: stream classifier (message / activity / drop)
// ---------------------------------------------------------------------------

export type StreamRole = 'user' | 'assistant' | 'thinking' | 'system';

export interface MessageItem {
  type: 'message';
  id: string;
  eventId: string;
  role: StreamRole;
  model: string | null;
  text: string;
  timestamp: string;
  /** True when the source event is on a Task-tool sidechain (subagent). For a
   *  user_message this means the orchestrator's prompt TO a subagent — NOT human
   *  input — so it must not be labelled "You" nor right-aligned. */
  sidechain: boolean;
  /** Caller classification of a user_message (role==='user'). CC folds three
   *  things into type:"user": typed input, slash-command/skill scaffolding the
   *  user INVOKED, and command output. All are user-originated (kept on the user
   *  side), but the origin drives the label/chip and whether the body collapses
   *  so injected text never reads as the user's own words. Optional so hand-built
   *  fixtures may omit it (treated as 'human'); buildStreamModel always sets it. */
  origin?: MessageOrigin;
  /** The invoked command (e.g. "/model") when origin==='command', else null. */
  commandName?: string | null;
}

export interface ActivityEvent {
  event: ObservedEventDto;
  result: { isError: boolean } | null;
  /** Tool execution time (ms) = matched tool_result.observed_at − tool_call
   *  .observed_at. null when there is no matched result (or not a tool_call).
   *  Optional so hand-built fixtures may omit it; buildStreamModel always sets
   *  it. hook_event duration lives in its own payload (see hookFacet), not here. */
  durationMs?: number | null;
}

export interface ActivityRun {
  type: 'activity-run';
  id: string;
  events: ActivityEvent[];
}

/** A contiguous run of sidechain (subagent) events — the prompt the orchestrator
 *  sent, the subagent's replies, and its tool activity — grouped so the whole
 *  exchange reads as one indented block separate from the main conversation.
 *  Parallel Task dispatches interleave their events chronologically, so a
 *  change of `agent_id` (subagent jsonl attribution, exposed on the events DTO)
 *  is ALSO a group boundary — contiguity alone would merge two agents' work. */
export interface SidechainGroup {
  type: 'sidechain-group';
  id: string;
  /** subagent attribution of the grouped events; null on pre-0023 ingests
   *  (no agentId in the DTO) where grouping falls back to contiguity only. */
  agentId: string | null;
  /** Agent type (e.g. "Explore") from the subagent_meta sidecar event, falling
   *  back to the sidechain assistant payload's attribution_agent. null when
   *  neither is observed (older ingests / sidecar absent). */
  agentType: string | null;
  /** The dispatching Task call's human-readable description from the sidecar.
   *  Preferred over the prompt's first line as the group's one-line identity. */
  description: string | null;
  /** event_id of the main-chain Task tool_call that spawned this agent —
   *  joined via the sidecar's toolUseId. null when the sidecar is absent or
   *  the Task call is outside the loaded window (jump unavailable). */
  taskEventId: string | null;
  /** 그 agent의 마지막 assistant_message 요약 — 축약 줄의 "결론". null=미관측/진행중. */
  conclusion: string | null;
  items: StreamItem[];
}

/** 한 디스패치 턴(같은 message_id)에서 병렬로 띄운 형제 서브에이전트 묶음. 동시성은
 *  에이전트 사이에만 있고 각 자식은 직렬이므로, agent_id로 전역 수집한 SidechainGroup을
 *  배치로 래핑한다(시간축은 배치=한 슬롯으로 보존). 단일 디스패치(N=1)는 래핑하지 않는다. */
export interface BatchGroup {
  type: 'batch-group';
  id: string;
  agentGroups: SidechainGroup[];
  /** 배치 후 main의 첫 assistant_message 요약 = 종합 결과. null=진행 중/미관측. */
  synthesis: string | null;
  /** 전부 완료 추정(모든 자식이 결론 보유)이면 true. 스트리밍 중 false. */
  settled: boolean;
}

/** Caller linkage harvested from `attachment_meta`/`subagent_meta` sidecar
 *  events (real layout: `<session>/subagents/agent-<id>.meta.json`, frozen in
 *  tests/fixtures/.../subagent_sidecar_v01 — sample 1, CC 2.1.176). */
interface SubagentMeta {
  agentType: string | null;
  description: string | null;
  toolUseId: string | null;
}

/** One redacted (content-less) thinking event. Claude Code transcripts store
 *  thinking blocks with an empty `thinking` text and an opaque `signature`
 *  only — the plaintext reasoning is recorded nowhere (not transcript, not
 *  OTel). So we can surface only that reasoning OCCURRED, when, and a rough
 *  size proxy from the signature length. We never attempt to decode it. */
export interface ThinkingEntry {
  eventId: string;
  timestamp: string;
  /** length of the encrypted `signature` — a rough proxy for reasoning size. */
  sigLen: number;
  /** request_id of the LLM response this thinking belongs to (join key). */
  requestId: string | null;
  /** per-response metrics (duration, tokens, …) joined via requestId. */
  metrics: LlmRequestMetrics | null;
}

/** One redacted thinking event, shown as a single compact, selectable inline
 *  marker in the conversation flow rather than buried in the activity stack.
 *  Selecting it surfaces the full per-response metrics in the side panel. */
export interface ThinkingMarker {
  type: 'thinking';
  id: string;
  events: ThinkingEntry[];
}

export type StreamItem = MessageItem | ActivityRun | SidechainGroup | ThinkingMarker | BatchGroup;

function userText(p: Record<string, unknown>): string {
  return (typeof p.content === 'string'
    ? p.content
    : typeof p.text === 'string'
    ? p.text
    : ''
  ).trim();
}

/** log_record event_names that represent genuine state changes and should
 *  appear as beats in the message view. All other log_record names are
 *  telemetry / facet observations and are dropped from the stream. */
// log_record event_names that are standalone, unique session beats worth
// showing in the message view. Excludes telemetry/duplicates: api_request,
// tool_decision, tool_result (folded into their owner entity's detail);
// user_prompt (duplicate of user_message); hook_execution_start/complete
// (redundant with the transcript hook_event already shown, and very high
// volume — would flood the stream).
const STREAM_STATE_LOG = new Set([
  'compaction',
  'skill_activated',
  'permission_mode_changed',
  'mcp_server_connection',
  'subagent_completed',
  'at_mention',
  'feedback_survey',
]);

function classify(
  e: ObservedEventDto,
): {
  cat: 'message' | 'activity' | 'drop' | 'thinking';
  role?: StreamRole;
  text?: string;
  model?: string | null;
  sigLen?: number;
  origin?: MessageOrigin;
  commandName?: string | null;
} {
  const p = asObj(e.payload);
  if (e.kind === 'user_message') {
    const t = userText(p);
    if (t === '') return { cat: 'drop' };
    // Everything CC folds into type:"user" is user-ORIGINATED (the user typed it
    // or invoked the command/skill that injected it), so it stays a user-side
    // message — never relocated to the agent/activity side. The origin only
    // drives how it's labelled and whether the body collapses (so an injected
    // skill body does not masquerade as the user's typed words).
    const { origin, commandName } = messageOrigin(e);
    return { cat: 'message', role: 'user', text: t, model: null, origin, commandName };
  }
  if (e.kind === 'assistant_message') {
    const t = (typeof p.text === 'string' ? p.text : '').trim();
    return t
      ? { cat: 'message', role: 'assistant', text: t, model: (p.model as string) ?? null }
      : { cat: 'drop' };
  }
  if (e.kind === 'thinking') {
    const t = (typeof p.thinking === 'string' ? p.thinking : '').trim();
    if (t) return { cat: 'message', role: 'thinking', text: t, model: null };
    // Redacted thinking: no plaintext anywhere, only an encrypted signature.
    // Surface it as a compact marker (not activity) so reasoning stays visible.
    const sig = typeof p.signature === 'string' ? p.signature : '';
    return { cat: 'thinking', sigLen: sig.length };
  }
  if (e.kind === 'system_summary') {
    // system_summary is heterogeneous. `subkind` here is the real CC transcript
    // `type:"system"` `subtype`, passed through verbatim by the Rust ingest
    // (`src/ingest/mapping.rs`: `e.subkind = s.subtype`). These literals are
    // real-data-anchored, observed in live CC sessions (e.g. 01fe9550):
    //   away_summary (recap, has `content`), turn_duration, stop_hook_summary,
    //   local_command — and across other sessions compact_boundary
    //   (`content:"Conversation compacted"` + `compactMetadata`). Not guessed.
    // Only the insightful, content-bearing subkinds are card-worthy in the
    // message view: away_summary (a CC work recap) and compact_boundary. Every
    // other subkind — and any away_summary/compact_boundary with empty content
    // — is meaningful data but not card-worthy → dropped (mirroring how empty
    // user_message/assistant_message drop).
    const sk = e.subkind ?? '';
    if (sk === 'away_summary' || sk === 'compact_boundary') {
      const content = typeof p.content === 'string' ? p.content.trim() : '';
      if (content === '') return { cat: 'drop' };
      return { cat: 'message', role: 'system', text: content, model: null };
    }
    return { cat: 'drop' };
  }
  // attachment_meta (file / deferred_tools_delta metadata) and session_state
  // (leafUuid / permissionMode) carry no display signal → not shown in the
  // message view.
  if (e.kind === 'attachment_meta' || e.kind === 'session_state') return { cat: 'drop' };
  if (e.kind === 'metric_sample' || e.kind === 'otel_span') return { cat: 'drop' };
  if (e.kind === 'log_record') {
    const name = (asObj(e.payload).event_name as string) ?? '';
    return STREAM_STATE_LOG.has(name) ? { cat: 'activity' } : { cat: 'drop' };
  }
  // hook_additional_context is a byte-identical duplicate of its hook_success
  // sibling (its `content` === hook_success.stdout.hookSpecificOutput
  // .additionalContext, verified against real data). It carries no execution
  // metadata and adds no information → drop it from the message view; the
  // injected context is still visible in hook_success's raw stdout.
  if (e.kind === 'hook_event' && e.subkind === 'hook_additional_context') {
    return { cat: 'drop' };
  }
  return { cat: 'activity' };
}

export function buildStreamModel(
  events: ObservedEventDto[],
  metricsByReq?: Map<string, LlmRequestMetrics>,
): StreamItem[] {
  const resultByUse = new Map<string, ObservedEventDto>();
  // Caller-linkage prepass: sidecar meta per agent, tool_call event ids per
  // tool_use_id (jump target lookup), and the attribution_agent fallback from
  // sidechain assistant payloads (secondary evidence when no sidecar landed).
  const metaByAgent = new Map<string, SubagentMeta>();
  const callEventByUse = new Map<string, string>();
  // The dispatching tool_call's message_id per tool_use_id. Batch membership =
  // siblings whose Task calls share a message_id (one assistant turn dispatched
  // them together); joined back from a child's sidecar toolUseId.
  const callMsgByUse = new Map<string, string | null>();
  const attributionByAgent = new Map<string, string>();
  for (const e of events) {
    if (e.kind === 'tool_result' && e.tool_use_id) resultByUse.set(e.tool_use_id, e);
    if (e.kind === 'tool_call' && e.tool_use_id) {
      callEventByUse.set(e.tool_use_id, e.event_id);
      callMsgByUse.set(e.tool_use_id, e.message_id ?? null);
    }
    const agent = e.agent_id || null;
    if (!agent) continue;
    const p = asObj(e.payload);
    if (e.kind === 'attachment_meta' && e.subkind === 'subagent_meta') {
      metaByAgent.set(agent, {
        agentType: typeof p.agentType === 'string' ? p.agentType : null,
        description: typeof p.description === 'string' ? p.description : null,
        toolUseId: typeof p.toolUseId === 'string' ? p.toolUseId : null,
      });
    } else if (
      e.kind === 'assistant_message' &&
      typeof p.attribution_agent === 'string' &&
      !attributionByAgent.has(agent)
    ) {
      attributionByAgent.set(agent, p.attribution_agent);
    }
  }

  const items: StreamItem[] = [];

  // Parallel Task dispatches interleave their sidechain events by timestamp, so
  // we collect GLOBALLY per agent_id (de-interleave) rather than breaking the
  // buffer on every agent change. Each agent's events accumulate into its own
  // buffer keyed by agent_id; `scOrder` preserves first-seen order so flushed
  // groups stay in dispatch order. The whole map flushes the moment the stream
  // returns to the main thread (or ends) — concurrency is between agents only,
  // each agent's own sub-stream is serial, so no ordering is lost. Missing
  // attribution (null/'' — pre-0023 ingest) never splits: it attaches to the
  // last-seen agent's buffer (contiguity fallback), keyed by NULL_AGENT_KEY
  // when no agent has been seen yet.
  const NULL_AGENT_KEY = '∅';
  const scBufs = new Map<string, StreamItem[]>();
  const scFirstIdByKey = new Map<string, string>();
  const scOrder: string[] = [];
  let lastScAgent: string | null = null;
  /** Build one SidechainGroup from an accumulated per-agent buffer. */
  const makeSidechainGroup = (key: string, buf: StreamItem[]): SidechainGroup => {
    const agentId = key === NULL_AGENT_KEY ? null : key;
    const meta = agentId ? metaByAgent.get(agentId) ?? null : null;
    const taskEventId = meta?.toolUseId ? callEventByUse.get(meta.toolUseId) ?? null : null;
    // Conclusion = the agent's LAST non-empty assistant_message (the design's
    // observed invariant: every sidechain agent ends on an assistant_message).
    // Truncated to a one-line preview length for the collapsed summary row.
    let conclusion: string | null = null;
    for (const it of buf) {
      if (it.type === 'message' && it.role === 'assistant' && it.text.trim()) {
        conclusion = it.text.trim().slice(0, 200);
      }
    }
    return {
      type: 'sidechain-group',
      id: `sc-${scFirstIdByKey.get(key) ?? buf[0]?.id ?? key}`,
      agentId,
      agentType: meta?.agentType ?? (agentId ? attributionByAgent.get(agentId) ?? null : null),
      description: meta?.description ?? null,
      taskEventId,
      conclusion,
      items: buf,
    };
  };
  // Batches awaiting their synthesis line (the first main assistant_message
  // after the batch returns). Filled lazily when that message is emitted.
  const pendingSynthesis: BatchGroup[] = [];
  const flushSidechain = () => {
    // 1) Materialize one SidechainGroup per accumulated agent buffer (order
    //    preserved by scOrder).
    const groups: SidechainGroup[] = [];
    for (const key of scOrder) {
      const buf = scBufs.get(key);
      if (!buf || !buf.length) continue;
      groups.push(makeSidechainGroup(key, buf));
    }
    scBufs.clear();
    scFirstIdByKey.clear();
    scOrder.length = 0;
    lastScAgent = null;
    if (!groups.length) return;

    // 2) Group siblings by their dispatching message_id (same assistant turn).
    //    A child's dispatch message_id = its sidecar toolUseId → callMsgByUse.
    //    Siblings with no resolvable message_id (no sidecar / pre-0023) get a
    //    unique solo key so they never merge — degrade to a plain SidechainGroup.
    const byMsg = new Map<string, SidechainGroup[]>();
    const msgOrder: string[] = [];
    for (const g of groups) {
      const tu = g.agentId ? metaByAgent.get(g.agentId)?.toolUseId ?? null : null;
      const mid = tu ? callMsgByUse.get(tu) ?? null : null;
      const key = mid ?? `solo-${g.id}`;
      let bucket = byMsg.get(key);
      if (!bucket) {
        bucket = [];
        byMsg.set(key, bucket);
        msgOrder.push(key);
      }
      bucket.push(g);
    }

    // 3) N>=2 siblings → BatchGroup (wraps the parallel siblings into one
    //    chronological slot). N==1 → the bare SidechainGroup (no wrapper).
    for (const key of msgOrder) {
      const sibs = byMsg.get(key)!;
      if (sibs.length >= 2) {
        const batch: BatchGroup = {
          type: 'batch-group',
          id: `batch-${sibs[0].id}`,
          agentGroups: sibs,
          synthesis: null,
          settled: sibs.every((s) => s.conclusion != null),
        };
        items.push(batch);
        pendingSynthesis.push(batch);
      } else {
        items.push(sibs[0]);
      }
    }
  };
  /** Fill any pending batch's synthesis from the first main assistant_message
   *  after the batch returned, then clear the queue (one synthesis per flush). */
  const fillPendingSynthesis = (text: string) => {
    if (!pendingSynthesis.length) return;
    const preview = text.trim().slice(0, 200);
    for (const b of pendingSynthesis) {
      if (b.synthesis == null) b.synthesis = preview;
    }
    pendingSynthesis.length = 0;
  };
  const emitSidechain = (it: StreamItem, agentId: string | null) => {
    const key = agentId ?? lastScAgent ?? NULL_AGENT_KEY;
    let buf = scBufs.get(key);
    if (!buf) {
      buf = [];
      scBufs.set(key, buf);
      scFirstIdByKey.set(key, it.id);
      scOrder.push(key);
    }
    buf.push(it);
    if (agentId) lastScAgent = agentId;
  };
  const emit = (it: StreamItem, sidechain: boolean, agentId: string | null) => {
    if (sidechain) {
      emitSidechain(it, agentId);
    } else {
      flushSidechain();
      items.push(it);
    }
  };

  let run: ActivityEvent[] = [];
  let runSc = false;
  let runAgent: string | null = null;
  const flush = () => {
    if (run.length) {
      emit({ type: 'activity-run', id: `run-${run[0].event.event_id}`, events: run }, runSc, runAgent);
      run = [];
      runAgent = null;
    }
  };

  for (const e of events) {
    if (e.kind === 'tool_result') continue;
    const sc = !!e.is_sidechain;
    const agent = e.agent_id || null; // '' (NULL TEXT row mapping) → null
    const c = classify(e);
    if (c.cat === 'message') {
      flush();
      emit(
        {
          type: 'message',
          id: e.event_id,
          eventId: e.event_id,
          role: c.role!,
          model: c.model ?? null,
          text: c.text!,
          timestamp: e.observed_at,
          sidechain: sc,
          origin: c.origin ?? 'human',
          commandName: c.commandName ?? null,
        },
        sc,
        agent,
      );
      // The first main assistant_message after a batch returns is its synthesis
      // (the design's "종합 결과" lifted to the batch's L0 line). The preceding
      // emit() already flushed any pending batch, so pendingSynthesis is set.
      if (!sc && c.role === 'assistant') fillPendingSynthesis(c.text!);
    } else if (c.cat === 'thinking') {
      // Close any open activity run first so order stays chronological. Each
      // redacted thinking is one LLM response → its own selectable marker
      // (no merging), so the side panel shows that one response's metrics.
      flush();
      const requestId = e.request_id ?? null;
      const metrics = requestId ? metricsByReq?.get(requestId) ?? null : null;
      emit(
        {
          type: 'thinking',
          id: `th-${e.event_id}`,
          events: [
            {
              eventId: e.event_id,
              timestamp: e.observed_at,
              sigLen: c.sigLen ?? 0,
              requestId,
              metrics,
            },
          ],
        },
        sc,
        agent,
      );
    } else if (c.cat === 'activity') {
      // A change of sidechain status — or of sidechain agent attribution —
      // breaks the activity run so a run never straddles the main↔subagent
      // boundary nor mixes two parallel subagents' tools.
      if (run.length && (runSc !== sc || (sc && agent && runAgent && agent !== runAgent))) flush();
      runSc = sc;
      runAgent = runAgent ?? agent;
      let result: { isError: boolean } | null = null;
      let durationMs: number | null = null;
      if (e.kind === 'tool_call' && e.tool_use_id) {
        const r = resultByUse.get(e.tool_use_id);
        if (r) {
          result = { isError: asObj(asObj(r.payload).tool_result).is_error === true };
          const ms = new Date(r.observed_at).getTime() - new Date(e.observed_at).getTime();
          durationMs = Number.isFinite(ms) ? ms : null;
        }
      }
      run.push({ event: e, result, durationMs });
    }
    // cat === 'drop': skip silently
  }
  flush();
  flushSidechain();
  return items;
}

