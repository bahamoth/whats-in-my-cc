// webui/src/components/replay/stream/streamModel.ts
import type { ObservedEventDto } from '../../../api/types';
import type { LlmRequestMetrics } from './llmRequestMetrics';
import { messageOrigin, type MessageOrigin } from './messageOrigin';
import { agentColor } from '../../../lib/colorHash';

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

/** Workflow tool_call의 `payload.input.script`에 박힌 `meta = {name, description}`
 *  리터럴에서 이름·설명만 끌어낸다. 스크립트 전체 평가가 아니라 표층 정규식 —
 *  meta는 순수 리터럴 규약(작은/큰따옴표만)이라 충분하다. 못 찾으면 null. */
export function parseWorkflowMeta(script: string): {
  name: string | null;
  description: string | null;
} {
  const pick = (key: string): string | null => {
    const m = script.match(new RegExp(key + "\\s*:\\s*['\"]([^'\"]*)['\"]"));
    return m ? m[1] : null;
  };
  return { name: pick('name'), description: pick('description') };
}

/** A harness `<task-notification>` (injected as a user_message) announcing a
 *  background task's completion. Its `<tool-use-id>` is the DETERMINISTIC join
 *  key back to the dispatching tool_call (Workflow / Agent run_in_background /
 *  background Bash) — verified against the real transcript (2026-06-14). Returns
 *  null when the text is not a task-notification. */
export function parseTaskNotification(content: string): {
  taskId: string | null;
  toolUseId: string | null;
  status: string | null;
  summary: string | null;
} | null {
  if (!content.includes('<task-notification>')) return null;
  const pick = (tag: string): string | null => {
    const m = content.match(new RegExp(`<${tag}>([\\s\\S]*?)</${tag}>`));
    return m ? m[1].trim() : null;
  };
  return {
    taskId: pick('task-id'),
    toolUseId: pick('tool-use-id'),
    status: pick('status'),
    summary: pick('summary'),
  };
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
  /** Number of background SUBAGENTS running concurrently while this MAIN message
   *  happened (their run span covers this message's timestamp). undefined/0 when
   *  none. Drives the "서브에이전트 N개 동시 실행" marker — the subject is the
   *  OTHER agents, not this message (annotateConcurrency). */
  concurrentBackground?: number;
  /** Set on a `<task-notification>` message whose <tool-use-id> matched a
   *  dispatching tool_call that is NOT a workflow/subagent (e.g. a background
   *  Bash) — so the noti can render as a clean "what finished" card and jump to
   *  the originating command (syncTaskNotifications). */
  notification?: {
    status: string | null;
    summary: string | null;
    /** the dispatching tool (e.g. "Bash") + a short command label. */
    toolLabel: string | null;
    /** event_id of the originating tool_call (jump target), null if not loaded. */
    callEventId: string | null;
  };
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
  /** 이 블록 실행 span과 겹친 main 메시지 수(백그라운드 동시진행) — annotateConcurrency. */
  concurrentMainCount?: number;
  /** insertSubagentEndCards가 이 그룹에 끝 카드를 붙였으면 true. 그러면 결론은 끝
   *  카드가 담당하므로 시작 카드(헤더)에선 결론 줄을 숨긴다(요청→결과 분리). */
  hasEndCard?: boolean;
  /** 매칭된 `<task-notification>`의 status(completed/failed/killed) — Agent
   *  run_in_background면 종료 알림에서 결정론적으로 채워진다(syncTaskNotifications). */
  endStatus?: string;
  /** 그 종료 알림 메시지의 event_id (점프 타깃). */
  notificationEventId?: string;
  items: StreamItem[];
}

/** 한 디스패치 턴(같은 message_id)에서 병렬로 띄운 형제 서브에이전트 묶음. 동시성은
 *  에이전트 사이에만 있고 각 자식은 직렬이므로, agent_id로 전역 수집한 SidechainGroup을
 *  배치로 래핑한다(시간축은 배치=한 슬롯으로 보존). 단일 디스패치(N=1)는 래핑하지 않는다. */
export interface BatchGroup {
  type: 'batch-group';
  id: string;
  agentGroups: SidechainGroup[];
  /** 디스패치 message_id — 백그라운드 배치가 main 메시지에 끊겨 여러 조각으로
   *  flush됐을 때 mergeConcurrentGroups가 같은 배치로 합치는 키. null=미상. */
  dispatchMessageId: string | null;
  /** 배치 후 main의 첫 assistant_message 요약 = 종합 결과. null=진행 중/미관측. */
  synthesis: string | null;
  /** 전부 완료 추정(모든 자식이 결론 보유)이면 true. 스트리밍 중 false. */
  settled: boolean;
  /** 이 배치 실행 span과 겹친 main 메시지 수(백그라운드 동시진행). */
  concurrentMainCount?: number;
}

/** `Workflow` 툴이 띄운 fan-out 서브에이전트 묶음. main 체인엔 Workflow tool_call
 *  1개 + turn_id만 남으므로(사이드카·per-agent tool_call 없음) 같은 turn의
 *  Workflow call로 묶는다. 동시성은 에이전트 사이에만, 각 자식은 직렬. */
export interface WorkflowGroup {
  type: 'workflow-group';
  id: string;
  /** meta.name (없으면 null → 컴포넌트가 '워크플로우'로 표기). */
  name: string | null;
  description: string | null;
  /** 디스패치한 Workflow tool_call의 event_id (점프 타깃). */
  taskEventId: string | null;
  agentGroups: SidechainGroup[];
  /** 실행 종료 후 main의 첫 assistant_message = 종합. null=진행 중/미관측. */
  synthesis: string | null;
  /** 모든 자식이 결론 보유면 true. */
  settled: boolean;
  /** 이 워크플로우 실행 span과 겹친 main 메시지 수(백그라운드 동시진행). */
  concurrentMainCount?: number;
  /** 매칭된 `<task-notification>`의 status/summary/시각 — 워크플로우 종료를
   *  결정론적으로 잇는다(추정 synthesis 대체). syncTaskNotifications. */
  endStatus?: string;
  endSummary?: string;
  endTimestamp?: string;
  /** 그 종료 알림 메시지의 event_id (점프 타깃). */
  notificationEventId?: string;
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

/** A contiguous run of ≥2 user-side scaffold messages (slash-command
 *  invocations, injected skill bodies, command output, system interrupts,
 *  harness task-notifications) folded into one collapsible block on the user
 *  side. CC injects these as type:"user" records the user TRIGGERED, so they
 *  belong on the user side, but a run of them buries the real conversation —
 *  this groups them so the main flow (human input + assistant) stays legible.
 *  Built by `groupScaffold` as a TOP-LEVEL post-pass; subagent/batch internals
 *  are never grouped (those are already collapsible). */
export interface ScaffoldGroup {
  type: 'scaffold-group';
  id: string;
  items: MessageItem[];
  /** The invoked command names (origin==='command') within the group, in
   *  order — drives the collapsed preview ("/chrome /claude-in-chrome …"). */
  commandNames: string[];
}

/** A compact "end card" for a background subagent — rendered as its own row at
 *  the chronological END of the agent's span (after the interleaved main rows),
 *  so the rail reads request(start card) → … → result(end card). Synthetic:
 *  carries no source event. Inserted by insertSubagentEndCards. */
export interface SubagentEndCard {
  type: 'subagent-end';
  id: string;
  agentId: string;
  /** hash(agent_id) color — same as the start block + gutter rail. */
  color: string;
  conclusion: string;
  durationMs: number;
  messageCount: number;
  toolCount: number;
  /** span end (max observed time) — drives row ordering / gutter coverage. */
  endTimestamp: string;
  /** matched `<task-notification>` status (Agent run_in_background) — deterministic
   *  completed/failed/killed; absent when the end is span-derived only. */
  status?: string;
  /** the completion notification's event_id (jump to 원문). */
  notificationEventId?: string;
}

/** A workflow's deterministic completion card, synced from its `<task-notification>`
 *  (joined by tool_use_id). Replaces the floating "알림" message at the position
 *  where the workflow finished — status + summary are authoritative (vs the
 *  heuristic synthesis). Synthetic row; see syncTaskNotifications. */
export interface WorkflowEndCard {
  type: 'workflow-end';
  id: string;
  /** workflow-group id this closes (for the gutter rail end marker). */
  workflowId: string;
  name: string | null;
  color: string;
  /** notification status — completed | failed | killed | … */
  status: string;
  summary: string | null;
  endTimestamp: string;
  agentCount: number;
  /** the completion notification's event_id (jump to 원문). */
  notificationEventId: string;
}

export type StreamItem =
  | MessageItem
  | ActivityRun
  | SidechainGroup
  | ThinkingMarker
  | BatchGroup
  | WorkflowGroup
  | ScaffoldGroup
  | SubagentEndCard
  | WorkflowEndCard;

/** One lane cell painted in the background-subagent gutter for a given row. */
export interface GutterCell {
  /** x-slot 0..2 (stable per agent for its whole life). */
  lane: number;
  agentId: string;
  /** hash(agent_id) color — shared with the SubagentGroup block header. */
  color: string;
  marker: 'start' | 'mid' | 'end';
}

/** Per-row gutter descriptor. `dense>0` ⇒ collapse to one neutral spine + count
 *  (≥4 background subagents overlap this row); otherwise render `cells`. */
export interface GutterRow {
  cells: GutterCell[];
  dense: number;
}

/** True for a TOP-LEVEL user-side scaffold message: a real user_message
 *  (not a subagent prompt) whose caller classification is anything but a typed
 *  human turn. `origin` is optional on hand-built fixtures (treated as human),
 *  so an undefined/`'human'` origin never qualifies. */
function isUserScaffold(it: StreamItem): it is MessageItem {
  return (
    it.type === 'message' &&
    it.role === 'user' &&
    !it.sidechain &&
    it.origin != null &&
    it.origin !== 'human'
  );
}

/** Fold each contiguous run of ≥2 top-level user-side scaffold messages into a
 *  ScaffoldGroup; a single scaffold message stays inline (no needless wrapper).
 *  Any non-scaffold item — a human message, assistant/thinking message,
 *  activity-run, sidechain-group, batch-group — breaks the run. Operates on the
 *  TOP-LEVEL items only; nested group internals are left untouched. */
export function groupScaffold(items: StreamItem[]): StreamItem[] {
  const out: StreamItem[] = [];
  let run: MessageItem[] = [];
  const flushRun = () => {
    if (run.length >= 2) {
      out.push({
        type: 'scaffold-group',
        id: `scaffold-${run[0].id}`,
        items: run,
        commandNames: run
          .filter((m) => m.origin === 'command' && m.commandName)
          .map((m) => m.commandName as string),
      });
    } else if (run.length === 1) {
      out.push(run[0]);
    }
    run = [];
  };
  for (const it of items) {
    if (isUserScaffold(it)) {
      run.push(it);
    } else {
      flushRun();
      out.push(it);
    }
  }
  flushRun();
  return out;
}

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

/** Background agents/workflows whose run straddles main messages get flushed into
 *  MULTIPLE groups (one per main-message-bounded segment). The source is ONE
 *  sequential thread per agent (verified: a subagent's own jsonl is a linear
 *  parentUuid chain), so merge the fragments back into one coherent block anchored
 *  at the FIRST (dispatch) position. Interleaved main messages keep their place →
 *  they flow AFTER the block (anchor-at-dispatch; precise cross-thread interleave
 *  is a separate waterfall view's job). Merge keys: sidechain-group→agent_id,
 *  workflow-group→id(=wf-<runId>), batch-group→dispatchMessageId. */
export function mergeConcurrentGroups(items: StreamItem[]): StreamItem[] {
  const lastAsst = (its: StreamItem[]): string | null => {
    let c: string | null = null;
    for (const it of its)
      if (it.type === 'message' && it.role === 'assistant' && it.text.trim())
        c = it.text.trim().slice(0, 200);
    return c;
  };
  const mergeSc = (into: SidechainGroup, more: SidechainGroup) => {
    into.items.push(...more.items);
    const c = lastAsst(more.items);
    if (c) into.conclusion = c;
  };
  const mergeChildren = (target: SidechainGroup[], incoming: SidechainGroup[]) => {
    for (const g of incoming) {
      const ex = g.agentId ? target.find((t) => t.agentId === g.agentId) : undefined;
      if (ex) mergeSc(ex, g);
      else target.push(g);
    }
  };
  const firstByKey = new Map<string, StreamItem>();
  const out: StreamItem[] = [];
  for (const it of items) {
    let key: string | null = null;
    if (it.type === 'sidechain-group' && it.agentId) key = `sc:${it.agentId}`;
    else if (it.type === 'workflow-group') key = `wf:${it.id}`;
    else if (it.type === 'batch-group' && it.dispatchMessageId) key = `batch:${it.dispatchMessageId}`;
    if (key == null) {
      out.push(it);
      continue;
    }
    const ex = firstByKey.get(key);
    if (!ex) {
      firstByKey.set(key, it);
      out.push(it);
      continue;
    }
    // Merge `it` into the dispatch-anchored first block `ex`; drop `it`.
    if (ex.type === 'sidechain-group' && it.type === 'sidechain-group') {
      mergeSc(ex, it);
    } else if (ex.type === 'workflow-group' && it.type === 'workflow-group') {
      mergeChildren(ex.agentGroups, it.agentGroups);
      ex.settled = ex.agentGroups.every((s) => s.conclusion != null);
      if (!ex.synthesis) ex.synthesis = it.synthesis;
      if (!ex.name) ex.name = it.name;
      if (!ex.taskEventId) ex.taskEventId = it.taskEventId;
    } else if (ex.type === 'batch-group' && it.type === 'batch-group') {
      mergeChildren(ex.agentGroups, it.agentGroups);
      ex.settled = ex.agentGroups.every((s) => s.conclusion != null);
      if (!ex.synthesis) ex.synthesis = it.synthesis;
    }
  }
  return out;
}

/** Mark MAIN messages that ran concurrently with a background block, and count
 *  per block. A block's run span = [min,max] observed time over its (recursive)
 *  sidechain items; a top-level main message whose timestamp falls in that span
 *  happened while the block ran (anchor-at-dispatch puts those messages after the
 *  block, so the "백그라운드 실행 중" marker reads naturally). Foreground blocks
 *  (main blocked) have no main messages in span → count 0 → no marker/badge. */
export function annotateConcurrency(items: StreamItem[]): StreamItem[] {
  const tms = (iso: string): number | null => {
    const t = new Date(iso).getTime();
    return Number.isNaN(t) ? null : t;
  };
  const spanOf = (g: SidechainGroup): { s: number; e: number } | null => {
    let s = Infinity;
    let e = -Infinity;
    const see = (iso: string) => {
      const t = tms(iso);
      if (t != null) {
        s = Math.min(s, t);
        e = Math.max(e, t);
      }
    };
    for (const it of g.items) {
      if (it.type === 'message') see(it.timestamp);
      else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
      else if (it.type === 'thinking') for (const ev of it.events) see(ev.timestamp);
    }
    return e > s ? { s, e } : null;
  };
  const blocks: { s: number; e: number; group: SidechainGroup | BatchGroup | WorkflowGroup }[] = [];
  const agentSpans: { s: number; e: number }[] = [];
  for (const it of items) {
    if (it.type !== 'sidechain-group' && it.type !== 'batch-group' && it.type !== 'workflow-group')
      continue;
    const scs: SidechainGroup[] = it.type === 'sidechain-group' ? [it] : it.agentGroups;
    let bs = Infinity;
    let be = -Infinity;
    for (const g of scs) {
      const sp = spanOf(g);
      if (sp) {
        agentSpans.push(sp); // per-subagent span (message-side count)
        bs = Math.min(bs, sp.s);
        be = Math.max(be, sp.e);
      }
    }
    if (be > bs) blocks.push({ s: bs, e: be, group: it });
  }
  if (!blocks.length) return items;
  for (const it of items) {
    if (it.type !== 'message' || it.sidechain) continue;
    const t = tms(it.timestamp);
    if (t == null) continue;
    // message-side: how many SUBAGENTS were running while this main message happened
    let n = 0;
    for (const a of agentSpans) if (t >= a.s && t <= a.e) n++;
    if (n > 0) it.concurrentBackground = n;
    // block-side: count main messages that fell within each block's run span
    for (const b of blocks) {
      if (t >= b.s && t <= b.e) b.group.concurrentMainCount = (b.group.concurrentMainCount ?? 0) + 1;
    }
  }
  return items;
}

const MAX_LANES = 3;
/** workflow type color (orange) — gutter rail + end card accent for a workflow,
 *  distinct from the per-agent hash colors. Matches WorkflowGroup's --wimcc-lane-quality. */
const WORKFLOW_COLOR = '#ff8a4c';

interface TaskNoti {
  status: string | null;
  summary: string | null;
  endTimestamp: string;
  eventId: string;
}

/** Join `<task-notification>` completion events to their dispatching group via
 *  tool_use_id (the verified deterministic key): a WORKFLOW noti → enrich the
 *  WorkflowGroup + replace the floating noti message with a `workflow-end` card;
 *  an Agent run_in_background noti → enrich the SidechainGroup (status drives the
 *  subagent end card) + absorb (drop) the noti message. Unmatched notis (e.g.
 *  background Bash, or notis with no loaded group) stay as-is. Runs after
 *  annotateConcurrency, before groupScaffold (notis still top-level here). */
export function syncTaskNotifications(
  items: StreamItem[],
  notiByToolUse: Map<string, TaskNoti>,
  wfToolUseByEvent: Map<string, string>,
  agentDispatchToolUse: Map<string, string>,
  callMetaByUse: Map<string, { eventId: string; label: string }> = new Map(),
  wfNameByToolUse: Map<string, string | null> = new Map(),
  notiByTaskId: Map<string, TaskNoti> = new Map(),
): StreamItem[] {
  // invert: noti event_id → its tool_use_id (to enrich the staying noti message).
  const toolUseByNotiEvent = new Map<string, string>();
  for (const [tu, n] of notiByToolUse) toolUseByNotiEvent.set(n.eventId, tu);
  const wfEndByNotiEvent = new Map<string, WorkflowEndCard>();
  const absorbNotiEvents = new Set<string>();
  for (const it of items) {
    if (it.type === 'workflow-group') {
      const tu = it.taskEventId ? wfToolUseByEvent.get(it.taskEventId) : undefined;
      const noti = tu ? notiByToolUse.get(tu) : undefined;
      if (noti) {
        it.endStatus = noti.status ?? undefined;
        it.endSummary = noti.summary ?? undefined;
        it.endTimestamp = noti.endTimestamp;
        it.notificationEventId = noti.eventId;
        wfEndByNotiEvent.set(noti.eventId, {
          type: 'workflow-end',
          id: `wfend-${it.id}`,
          workflowId: it.id,
          name: it.name,
          color: WORKFLOW_COLOR,
          status: noti.status ?? 'completed',
          summary: noti.summary,
          endTimestamp: noti.endTimestamp,
          agentCount: it.agentGroups.length,
          notificationEventId: noti.eventId,
        });
      }
    } else if (it.type === 'sidechain-group' && it.agentId) {
      // Join the completion noti to this subagent by EITHER the dispatch
      // tool_use_id (needs the sidecar) OR — more robustly, sidecar-independent —
      // the noti's <task-id>, which for a background Agent IS the agent_id.
      const tu = agentDispatchToolUse.get(it.agentId);
      const noti = (tu ? notiByToolUse.get(tu) : undefined) ?? notiByTaskId.get(it.agentId);
      if (noti) {
        it.endStatus = noti.status ?? undefined;
        it.notificationEventId = noti.eventId;
        absorbNotiEvents.add(noti.eventId);
      }
    }
  }
  const out: StreamItem[] = [];
  for (const it of items) {
    if (it.type === 'message') {
      const wfEnd = wfEndByNotiEvent.get(it.eventId);
      if (wfEnd) {
        out.push(wfEnd);
        continue;
      }
      if (absorbNotiEvents.has(it.eventId)) continue; // absorbed into the subagent end card
      // a STAYING notification (no matched group): if it's a WORKFLOW noti that
      // produced no group (e.g. failed at launch / 0 agents), still surface a
      // workflow-end card so the failure is visible + connected. Otherwise (a
      // background command like Bash) enrich the message so it reads as "what
      // finished" + can jump to the originating command.
      const tu = toolUseByNotiEvent.get(it.eventId);
      if (tu && wfNameByToolUse.has(tu)) {
        const noti = notiByToolUse.get(tu)!;
        out.push({
          type: 'workflow-end',
          id: `wfend-noti-${it.eventId}`,
          workflowId: `nogroup-${tu}`,
          name: wfNameByToolUse.get(tu) ?? null,
          color: WORKFLOW_COLOR,
          status: noti.status ?? 'completed',
          summary: noti.summary,
          endTimestamp: noti.endTimestamp,
          agentCount: 0,
          notificationEventId: it.eventId,
        });
        continue;
      }
      if (tu) {
        const noti = notiByToolUse.get(tu)!;
        const call = callMetaByUse.get(tu);
        it.notification = {
          status: noti.status,
          summary: noti.summary,
          toolLabel: call?.label ?? null,
          callEventId: call?.eventId ?? null,
        };
      }
    }
    out.push(it);
  }
  return out;
}

/** Representative wall-clock (ms) of a top-level row — gutter coverage tests +
 *  the timeline spine's left time ruler (clockLabel). */
export function rowTimeMs(it: StreamItem): number | null {
  const t = (iso: string) => {
    const n = new Date(iso).getTime();
    return Number.isNaN(n) ? null : n;
  };
  if (it.type === 'message') return t(it.timestamp);
  if (it.type === 'subagent-end') return t(it.endTimestamp);
  if (it.type === 'workflow-end') return t(it.endTimestamp);
  if (it.type === 'activity-run') return it.events.length ? t(it.events[0].event.observed_at) : null;
  if (it.type === 'thinking') return it.events.length ? t(it.events[0].timestamp) : null;
  if (it.type === 'scaffold-group') return it.items.length ? t(it.items[0].timestamp) : null;
  // group containers: earliest child event
  const groups = it.type === 'sidechain-group' ? [it] : it.agentGroups;
  let min = Infinity;
  for (const g of groups)
    for (const c of g.items) {
      if (c.type === 'message') {
        const x = t(c.timestamp);
        if (x != null) min = Math.min(min, x);
      } else if (c.type === 'activity-run') {
        for (const ae of c.events) {
          const x = t(ae.event.observed_at);
          if (x != null) min = Math.min(min, x);
        }
      }
    }
  return Number.isFinite(min) ? min : null;
}

function sidechainSpan(g: SidechainGroup): { s: number; e: number } | null {
  let s = Infinity;
  let e = -Infinity;
  const see = (iso: string) => {
    const n = new Date(iso).getTime();
    if (!Number.isNaN(n)) {
      s = Math.min(s, n);
      e = Math.max(e, n);
    }
  };
  for (const it of g.items) {
    if (it.type === 'message') see(it.timestamp);
    else if (it.type === 'activity-run') for (const ae of it.events) see(ae.event.observed_at);
    else if (it.type === 'thinking') for (const ev of it.events) see(ev.timestamp);
  }
  return e > s ? { s, e } : null;
}

/** Per-row background-subagent gutter. Lanes come ONLY from TOP-LEVEL
 *  sidechain-groups (standalone background subagents); batch/workflow containers
 *  have their own viz and contribute no lanes. Greedy interval partitioning caps
 *  at 3 lanes (stable x per agent for its whole life); a row covered by an agent
 *  that could not get a lane (≥4 simultaneous) is `dense`. Markers: the agent's
 *  own block row = 'start', its last covered row = 'end', covered rows between =
 *  'mid'. Returns a Map keyed by row item.id (rows with no coverage are absent).*/
export function computeBgGutter(items: StreamItem[]): Map<string, GutterRow> {
  // A track = one bookended background unit (a standalone subagent OR a workflow).
  // blockId = its start card row; endRowId = its end card row (preferred) or last
  // covered row (fallback). color = per-agent hash, or the workflow orange.
  // blockIndex = the block row's position in `items`. Coverage is anchored to it:
  // a track only paints rows AT OR AFTER its block row, so the rail never bleeds
  // UP onto an earlier main-thread row that merely overlaps the subagent in time
  // (e.g. a main-thread thinking emitted in the dispatch turn, which is NOT the
  // subagent's work). The rail starts at the block (the subagent itself) and
  // extends DOWN over the concurrent main rows — the chosen "liveness span,
  // anchored at the subagent" model.
  type Track = { key: string; blockId: string; blockIndex: number; color: string; s: number; e: number; lane: number; endRowId: string | null };
  const tracks: Track[] = [];
  const byId = new Map(items.map((i) => [i.id, i] as const));
  const indexById = new Map(items.map((i, idx) => [i.id, idx] as const));
  const subEndIds = new Set(items.filter((i) => i.type === 'subagent-end').map((i) => i.id));
  const wfEndByWorkflowId = new Map<string, string>();
  for (const it of items) if (it.type === 'workflow-end') wfEndByWorkflowId.set(it.workflowId, it.id);

  for (const it of items) {
    if (it.type === 'sidechain-group' && it.agentId) {
      const sp = sidechainSpan(it);
      if (!sp) continue;
      const endId = `end-${it.agentId}`;
      tracks.push({ key: it.agentId, blockId: it.id, blockIndex: indexById.get(it.id)!, color: agentColor(it.agentId), s: sp.s, e: sp.e, lane: -1, endRowId: subEndIds.has(endId) ? endId : null });
    } else if (it.type === 'workflow-group') {
      const blockT = rowTimeMs(it);
      let s = blockT ?? Infinity;
      let e = -Infinity;
      for (const g of it.agentGroups) {
        const sp = sidechainSpan(g);
        if (sp) {
          s = Math.min(s, sp.s);
          e = Math.max(e, sp.e);
        }
      }
      const endId = wfEndByWorkflowId.get(it.id) ?? null;
      if (endId) {
        const et = rowTimeMs(byId.get(endId)!);
        if (et != null) e = Math.max(e, et);
      }
      if (!(e > s)) continue;
      tracks.push({ key: `wf-${it.id}`, blockId: it.id, blockIndex: indexById.get(it.id)!, color: WORKFLOW_COLOR, s, e, lane: -1, endRowId: endId });
    }
  }
  const out = new Map<string, GutterRow>();
  if (!tracks.length) return out;

  // greedy lane assignment (stable per track): sort by start, lowest free lane.
  tracks.sort((a, b) => a.s - b.s);
  const laneFreeAt = new Array(MAX_LANES).fill(-Infinity);
  for (const t of tracks) {
    for (let L = 0; L < MAX_LANES; L++) {
      if (t.s >= laneFreeAt[L]) {
        t.lane = L;
        laneFreeAt[L] = t.e;
        break;
      }
    }
  }

  // tracks without an explicit end card fall back to last-covered row for ✓.
  // Same anchor rule: only rows AT OR AFTER the block can be the end row.
  for (const t of tracks) {
    if (t.endRowId) continue;
    let last: string | null = null;
    for (let i = 0; i < items.length; i++) {
      if (i < t.blockIndex) continue;
      const tt = rowTimeMs(items[i]);
      if (tt != null && tt <= t.e) last = items[i].id;
    }
    t.endRowId = last;
  }

  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    const t = rowTimeMs(it);
    if (t == null) continue;
    // Anchor coverage to the block row: paint rows at/after the block whose time
    // is within the span. Rows above the block (even if time-overlapping) are not
    // the subagent's and stay unpainted.
    const covering = tracks.filter((a) => i >= a.blockIndex && t <= a.e);
    if (!covering.length) continue;
    const overflow = covering.some((a) => a.lane < 0);
    if (overflow) {
      out.set(it.id, { cells: [], dense: covering.length });
      continue;
    }
    const cells: GutterCell[] = covering.map((a) => ({
      lane: a.lane,
      agentId: a.key,
      color: a.color,
      marker: a.blockId === it.id ? 'start' : a.endRowId === it.id ? 'end' : 'mid',
    }));
    cells.sort((x, y) => x.lane - y.lane);
    out.set(it.id, { cells, dense: 0 });
  }
  return out;
}

/** Insert a synthetic `subagent-end` card at the chronological end of each
 *  TOP-LEVEL background subagent's span — but only when it (a) has a conclusion
 *  (i.e. finished) and (b) genuinely interleaved with ≥1 other top-level row in
 *  its span (the background case). Foreground/in-progress agents get none. The
 *  source group is flagged `hasEndCard` so its start card drops the (now
 *  duplicated) conclusion line. Run as the OUTERMOST pipeline pass. */
export function insertSubagentEndCards(items: StreamItem[]): StreamItem[] {
  const inserts = new Map<number, SubagentEndCard[]>();
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (it.type !== 'sidechain-group' || !it.agentId || !it.conclusion) continue;
    const sp = sidechainSpan(it);
    if (!sp) continue;
    let lastIdx = i;
    let interleaved = false;
    for (let j = i + 1; j < items.length; j++) {
      const t = rowTimeMs(items[j]);
      if (t == null) continue;
      if (t >= sp.s && t <= sp.e) {
        lastIdx = j;
        interleaved = true;
      }
    }
    if (!interleaved) continue;
    let messageCount = 0;
    let toolCount = 0;
    for (const c of it.items) {
      if (c.type === 'message') messageCount++;
      else if (c.type === 'activity-run') for (const ae of c.events) if (ae.event.kind === 'tool_call') toolCount++;
    }
    it.hasEndCard = true;
    const card: SubagentEndCard = {
      type: 'subagent-end',
      id: `end-${it.agentId}`,
      agentId: it.agentId,
      color: agentColor(it.agentId),
      conclusion: it.conclusion,
      durationMs: sp.e - sp.s,
      messageCount,
      toolCount,
      endTimestamp: new Date(sp.e).toISOString(),
      status: it.endStatus,
      notificationEventId: it.notificationEventId,
    };
    const arr = inserts.get(lastIdx) ?? [];
    arr.push(card);
    inserts.set(lastIdx, arr);
  }
  if (!inserts.size) return items;
  const out: StreamItem[] = [];
  for (let i = 0; i < items.length; i++) {
    out.push(items[i]);
    const cards = inserts.get(i);
    if (cards) out.push(...cards);
  }
  return out;
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
  // `<task-notification>` completion events keyed by their <tool-use-id> (the
  // deterministic join back to the dispatching Workflow/Agent/Bash call).
  const notiByToolUse = new Map<string, TaskNoti>();
  // …also keyed by <task-id>, which for a background Agent IS its agent_id — a
  // sidecar-independent join from a completion noti to its subagent group.
  const notiByTaskId = new Map<string, TaskNoti>();
  // tool_call event_id + short label per tool_use_id — lets a background-command
  // noti (e.g. Bash) say what finished and jump to the originating command.
  const callMetaByUse = new Map<string, { eventId: string; label: string }>();
  // Workflow tool_call들(이름 메타). 워크플로우가 띄운 에이전트는 events의
  // `workflow_run_id`(파일 경로 `…/subagents/workflows/<runId>/` 유래)로 결정론적으로
  // 묶는다. 그룹에 이름을 붙이려고 run_id ↔ Workflow tool_call을 잇는데, run_id는 call의
  // 입력이 아니라 그 tool_result 텍스트("Run ID: wf_…")에 있으므로 resultByUse 완성 후 매핑.
  const wfCalls: {
    eventId: string;
    toolUseId: string;
    name: string | null;
    description: string | null;
  }[] = [];
  for (const e of events) {
    if (e.kind === 'tool_result' && e.tool_use_id) resultByUse.set(e.tool_use_id, e);
    if (e.kind === 'tool_call' && e.tool_use_id) {
      callEventByUse.set(e.tool_use_id, e.event_id);
      callMsgByUse.set(e.tool_use_id, e.message_id ?? null);
      const inp = asObj(asObj(e.payload).input);
      const cmd = typeof inp.command === 'string' ? inp.command.split('\n')[0].trim() : null;
      const label = cmd ? `${e.tool_name ?? 'Bash'} · ${cmd.slice(0, 80)}` : e.tool_name ?? 'tool';
      callMetaByUse.set(e.tool_use_id, { eventId: e.event_id, label });
    }
    if (e.kind === 'tool_call' && e.tool_name === 'Workflow' && e.tool_use_id) {
      const input = asObj(asObj(e.payload).input);
      const meta = parseWorkflowMeta(typeof input.script === 'string' ? input.script : '');
      wfCalls.push({ eventId: e.event_id, toolUseId: e.tool_use_id, ...meta });
    }
    if (e.kind === 'user_message') {
      const pp = asObj(e.payload);
      const content = typeof pp.content === 'string' ? pp.content : typeof pp.text === 'string' ? pp.text : '';
      const noti = parseTaskNotification(content);
      if (noti) {
        const rec: TaskNoti = {
          status: noti.status,
          summary: noti.summary,
          endTimestamp: e.observed_at,
          eventId: e.event_id,
        };
        if (noti.toolUseId) notiByToolUse.set(noti.toolUseId, rec);
        if (noti.taskId) notiByTaskId.set(noti.taskId, rec);
      }
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

  // run_id → 워크플로우 정체성. Workflow call의 tool_result 텍스트에서 run id를 뽑아 잇는다.
  // (없으면 그룹 이름은 null로 degrade — 그룹핑 자체는 run_id로 항상 동작.)
  const wfRunMeta = new Map<
    string,
    { eventId: string; name: string | null; description: string | null }
  >();
  for (const c of wfCalls) {
    const res = resultByUse.get(c.toolUseId);
    if (!res) continue;
    const m = JSON.stringify(res.payload).match(/wf_[a-z0-9-]+/i);
    if (m) wfRunMeta.set(m[0], { eventId: c.eventId, name: c.name, description: c.description });
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
  // per-agent turn_id + 최초 시작 시각 — Workflow 귀속(turn_id)·시작순 정렬에 쓴다.
  // agent_id → workflow_run_id (events DTO, 파일 경로 유래). 워크플로우 fan-out의
  // 결정론적 그룹 키 — 첫 이벤트에서 캡처(같은 에이전트는 동일 run_id).
  const scRunByKey = new Map<string, string | null>();
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
  // The batch awaiting its synthesis line (the first main assistant_message
  // after the batch returns). At most one is ever pending — a flush produces
  // one batch slot, and the next main assistant_message consumes it — so a
  // single reference, not a queue, expresses the intent.
  let pendingSynthesis: BatchGroup | WorkflowGroup | null = null;
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

    // 2) Route each agent group: (a) 사이드카 message_id가 잡히면 Agent-배치,
    //    (b) 아니면 events의 `workflow_run_id`(파일 경로 유래)가 있으면 그 워크플로우
    //    실행, (c) 그 외엔 solo. 키는 추론(turn_id)이 아니라 하네스가 파일로 남긴
    //    run_id라 병렬·파이프라인 모두 정확하다.
    type Bucket = {
      kind: 'batch' | 'wf' | 'solo';
      wf?: { runId: string; eventId: string | null; name: string | null; description: string | null };
      sibs: SidechainGroup[];
    };
    const byKey = new Map<string, Bucket>();
    const keyOrder: string[] = [];
    const put = (key: string, kind: Bucket['kind'], g: SidechainGroup, wf?: Bucket['wf']) => {
      let b = byKey.get(key);
      if (!b) {
        b = { kind, sibs: [], wf };
        byKey.set(key, b);
        keyOrder.push(key);
      }
      b.sibs.push(g);
    };
    for (const g of groups) {
      const tu = g.agentId ? metaByAgent.get(g.agentId)?.toolUseId ?? null : null;
      const mid = tu ? callMsgByUse.get(tu) ?? null : null;
      if (mid) {
        put(`msg-${mid}`, 'batch', g);
        continue;
      }
      const runId = g.agentId ? scRunByKey.get(g.agentId) ?? null : null;
      if (runId) {
        const meta = wfRunMeta.get(runId);
        put(`wf-${runId}`, 'wf', g, {
          runId,
          eventId: meta?.eventId ?? null,
          name: meta?.name ?? null,
          description: meta?.description ?? null,
        });
        continue;
      }
      put(`solo-${g.id}`, 'solo', g);
    }

    // 3) Materialize: workflow → WorkflowGroup(자식 N>=1), batch(N>=2) → BatchGroup,
    //    그 외 → bare SidechainGroup. 가장 최근 flush된 그룹이 synthesis를 기다린다.
    for (const key of keyOrder) {
      const b = byKey.get(key)!;
      if (b.kind === 'wf') {
        const wg: WorkflowGroup = {
          type: 'workflow-group',
          id: `wf-${b.wf?.runId ?? b.sibs[0].id}`,
          name: b.wf?.name ?? null,
          description: b.wf?.description ?? null,
          taskEventId: b.wf?.eventId ?? null,
          agentGroups: b.sibs,
          synthesis: null,
          settled: b.sibs.every((s) => s.conclusion != null),
        };
        items.push(wg);
        pendingSynthesis = wg;
      } else if (b.kind === 'batch' && b.sibs.length >= 2) {
        const batch: BatchGroup = {
          type: 'batch-group',
          id: `batch-${b.sibs[0].id}`,
          agentGroups: b.sibs,
          dispatchMessageId: key.startsWith('msg-') ? key.slice(4) : null,
          synthesis: null,
          settled: b.sibs.every((s) => s.conclusion != null),
        };
        items.push(batch);
        pendingSynthesis = batch;
      } else {
        items.push(b.sibs[0]);
      }
    }
  };
  /** Fill the pending batch's synthesis from the first main assistant_message
   *  after the batch returned, then clear it (one synthesis per batch). */
  const fillPendingSynthesis = (text: string) => {
    if (!pendingSynthesis) return;
    if (pendingSynthesis.synthesis == null) pendingSynthesis.synthesis = text.trim().slice(0, 200);
    pendingSynthesis = null;
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
  // `flushMain` says whether a MAIN-chain item should first flush the open
  // sidechain buffers. Only a main MESSAGE means "main resumed" — its
  // tool activity / thinking can interleave a still-running parallel window
  // (real: fb6b8e3a main tool_calls at 12:34:02–07 mid-window) and MUST NOT
  // close the batch, or one agent splits into out-of-batch + in-batch shards
  // (the design §1 symptom). Interleaved main activity just emits in place;
  // the sidechain buffers stay open and flush on the next main message / end.
  const emit = (
    it: StreamItem,
    sidechain: boolean,
    agentId: string | null,
    flushMain: boolean,
  ) => {
    if (sidechain) {
      emitSidechain(it, agentId);
    } else {
      if (flushMain) flushSidechain();
      items.push(it);
    }
  };

  let run: ActivityEvent[] = [];
  let runSc = false;
  let runAgent: string | null = null;
  const flush = () => {
    if (run.length) {
      // An activity-run never signals main resumption → never flushes sidechain.
      emit({ type: 'activity-run', id: `run-${run[0].event.event_id}`, events: run }, runSc, runAgent, false);
      run = [];
      runAgent = null;
    }
  };

  for (const e of events) {
    if (e.kind === 'tool_result') continue;
    const sc = !!e.is_sidechain;
    const agent = e.agent_id || null; // '' (NULL TEXT row mapping) → null
    const c = classify(e);
    if (sc && agent && !scRunByKey.has(agent)) {
      scRunByKey.set(agent, e.workflow_run_id ?? null);
    }
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
        true, // a MAIN message signals main resumed → flush the open batch first
      );
      // The first main assistant_message after a batch returns is its synthesis
      // (the design's "종합 결과" lifted to the batch's L0 line). The preceding
      // emit(…, flushMain=true) already flushed the batch, so pendingBatch is set.
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
        false, // thinking can interleave a running window → never flushes batch
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
  // Top-level post-pass: fold contiguous user-side scaffold runs (commands,
  // skill bodies, command output, interrupts, task-notifications) into one
  // collapsible block so the main conversation stays legible. Subagent/batch
  // internals are untouched — groupScaffold scans only this top-level array.
  // Join keys for task-notification sync: WorkflowGroup.taskEventId → Workflow
  // call tool_use_id; SidechainGroup.agentId → dispatching Agent call tool_use_id.
  const wfToolUseByEvent = new Map<string, string>();
  for (const c of wfCalls) wfToolUseByEvent.set(c.eventId, c.toolUseId);
  const agentDispatchToolUse = new Map<string, string>();
  for (const [agentId, meta] of metaByAgent) if (meta.toolUseId) agentDispatchToolUse.set(agentId, meta.toolUseId);
  const wfNameByToolUse = new Map<string, string | null>();
  for (const c of wfCalls) wfNameByToolUse.set(c.toolUseId, c.name);

  return insertSubagentEndCards(
    groupScaffold(
      syncTaskNotifications(
        annotateConcurrency(mergeConcurrentGroups(items)),
        notiByToolUse,
        wfToolUseByEvent,
        agentDispatchToolUse,
        callMetaByUse,
        wfNameByToolUse,
        notiByTaskId,
      ),
    ),
  );
}

