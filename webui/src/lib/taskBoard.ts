// Task lifecycle board — correlate TaskCreate/TaskUpdate tool calls into a
// per-session todo board. PURE function over the event stream (the same SSOT a
// panel renders), so it is unit-testable without the API.
//
// Correlation key is the server-assigned numeric taskId:
//   - TaskCreate carries `subject` in its input; its taskId only appears in the
//     matched tool_result ("Task #N created successfully: <subject>").
//   - TaskUpdate carries `{ taskId, status }` in its input.
// TaskStop targets a *background* task (alphanumeric id, separate lifecycle) and
// is deliberately ignored here — it is not one of these numeric todos.

import type { ObservedEventDto } from '../api/types';

export type TaskTransition = { status: string; at: string; eventId: string };

export type TaskBoardEntry = {
  taskId: string;
  subject: string;
  /** event_id of the TaskCreate — lets the board jump into the replay. */
  eventId: string;
  createdAt: string;
  /** created → …updates, sorted by observed_at. First entry is always 'created'. */
  transitions: TaskTransition[];
  /** Latest observed status. */
  status: string;
  /** Last transition − createdAt, in ms. null when the task only has 'created'. */
  durationMs: number | null;
  /** Whether an explicit in_progress transition was ever observed. A task that
   *  jumped straight to completed (false) is a fact worth surfacing. */
  sawInProgress: boolean;
};

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function inputOf(e: ObservedEventDto): Record<string, unknown> {
  return asObj(asObj(e.payload).input);
}

/** Parse the server taskId out of a TaskCreate result line. */
function taskIdFromResult(content: string): string | null {
  const m = content.match(/Task #(\d+)\b/);
  return m ? m[1] : null;
}

export function buildTaskBoard(events: ObservedEventDto[]): TaskBoardEntry[] {
  // tool_use_id → tool_result content (for TaskCreate taskId resolution).
  const resultByUseId = new Map<string, string>();
  for (const e of events) {
    if (e.kind !== 'tool_result') continue;
    const tr = asObj(asObj(e.payload).tool_result);
    const useId = (tr.tool_use_id as string) || e.tool_use_id || '';
    const content = tr.content;
    if (useId && typeof content === 'string') resultByUseId.set(useId, content);
  }

  const byId = new Map<string, TaskBoardEntry>();

  // First pass: creates establish the entries (taskId resolved from result).
  for (const e of events) {
    if (e.kind !== 'tool_call' || e.tool_name !== 'TaskCreate') continue;
    const useId = e.tool_use_id || '';
    const result = resultByUseId.get(useId);
    const taskId = result ? taskIdFromResult(result) : null;
    if (!taskId) continue; // un-correlatable create — skip rather than guess
    if (byId.has(taskId)) continue;
    const subject = (inputOf(e).subject as string) ?? '';
    byId.set(taskId, {
      taskId,
      subject,
      eventId: e.event_id,
      createdAt: e.observed_at,
      transitions: [{ status: 'created', at: e.observed_at, eventId: e.event_id }],
      status: 'created',
      durationMs: null,
      sawInProgress: false,
    });
  }

  // Second pass: updates append transitions to their task.
  for (const e of events) {
    if (e.kind !== 'tool_call' || e.tool_name !== 'TaskUpdate') continue;
    const input = inputOf(e);
    const taskId = input.taskId != null ? String(input.taskId) : '';
    const status = input.status != null ? String(input.status) : '';
    const entry = byId.get(taskId);
    if (!entry || !status) continue;
    entry.transitions.push({ status, at: e.observed_at, eventId: e.event_id });
  }

  // Finalize each entry: sort transitions, derive status/duration/sawInProgress.
  const out: TaskBoardEntry[] = [];
  for (const entry of byId.values()) {
    entry.transitions.sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
    const last = entry.transitions[entry.transitions.length - 1];
    entry.status = last.status;
    entry.sawInProgress = entry.transitions.some((t) => t.status === 'in_progress');
    entry.durationMs =
      entry.transitions.length > 1 ? Date.parse(last.at) - Date.parse(entry.createdAt) : null;
    out.push(entry);
  }

  // Sort by numeric taskId ascending (fallback to string compare).
  out.sort((a, b) => {
    const na = Number(a.taskId);
    const nb = Number(b.taskId);
    if (Number.isFinite(na) && Number.isFinite(nb)) return na - nb;
    return a.taskId.localeCompare(b.taskId);
  });
  return out;
}
