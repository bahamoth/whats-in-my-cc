// webui/src/components/replay/stream/messageOrigin.ts
// SSOT for classifying the CALLER of a `type:"user"` transcript record. CC
// folds three things into `type:"user"`: genuine human input, slash-command /
// skill scaffolding injected on the user's behalf, and tool_result blocks. This
// helper separates them from DETERMINISTIC structural signals only (command
// markers + the `isMeta` flag) — no semantic inference — so it honours the
// "측정은 wimcc(결정론), 판별은 LLM" principle. Both the message view
// (streamModel.classify, MessageCard) and the activity stack (nodeLabel)
// consume it, so the same record is classified identically wherever it renders.
//
// Real-data anchored (sample 1, CC 2.1.176, entrypoint remote_mobile; frozen in
// tests/fixtures/transcripts/real/message_origin_v01.jsonl): human turns are
// isMeta=false with no command marker (a harness `<system-reminder>…` wrapper
// does NOT make them non-human); command/skill/output records carry a marker
// and/or isMeta=true. `isMeta` provenance: transcript `isMeta` →
// ObservedEvent.is_meta (src/ingest/mapping.rs), exposed on the events DTO.

export type MessageOrigin =
  | 'human' // a person typed it
  | 'command' // a slash-command invocation (<command-name> …)
  | 'command-output' // local-command stdout/caveat echoed back as a user record
  | 'skill' // skill / command body injected on the user's behalf (isMeta)
  | 'system' // a system beat folded into type:"user" (e.g. interrupt)
  | 'notification' // a harness background-task notice (<task-notification> …)
  | 'teammate'; // a teammate session's message relayed in (<teammate-message …>)

export interface OriginResult {
  origin: MessageOrigin;
  /** the invoked command (e.g. "/model") when origin === 'command', else null. */
  commandName: string | null;
  /** the sending teammate (teammate_id attr) when origin === 'teammate', else null. */
  teammateId?: string | null;
}

/** Leading markers CC writes into the user record text. Anchored at the start
 *  (after optional whitespace) so an incidental mention mid-text never trips. */
const COMMAND_INVOCATION = /^\s*(<command-name>|<command-message>|<command-args>)/;
const COMMAND_OUTPUT = /^\s*(<local-command-stdout>|<local-command-caveat>)/;
const SYSTEM_BEAT = /^\s*\[Request interrupted/;
const SKILL_SCAFFOLD = /^\s*Base directory for this skill:/;
// Harness-injected background-task completion notice, folded into type:"user".
// Anchored: 전 DB 55건 user_message가 <task-notification> 선행, isMeta 없음 —
// so the marker (not isMeta) is the deterministic signal that distinguishes it
// from a human turn (which would otherwise post as "You").
const NOTIFICATION = /^\s*<task-notification>/;
// Teammate 세션(CC 2.1.198, named Agent 스폰)의 메시지 — 리드 쪽은
// "Another Claude session sent a message:" 접두문 뒤, 팀메이트 쪽 인바운드는
// 접두문 없이 마커로 바로 시작한다 (teammate_v01 fixture, 표본 1). isMeta 없음
// — 마커가 유일한 결정론 신호.
const TEAMMATE_MESSAGE = /^\s*(?:Another Claude session sent a message:\s*)?<teammate-message[\s>]/;

/** Union of every non-human leading marker — kept so other surfaces can reuse
 *  the exact same set instead of re-listing it (the old per-file SCAFFOLD). */
export function hasScaffoldMarker(text: string): boolean {
  return (
    COMMAND_INVOCATION.test(text) ||
    COMMAND_OUTPUT.test(text) ||
    SYSTEM_BEAT.test(text) ||
    SKILL_SCAFFOLD.test(text) ||
    NOTIFICATION.test(text) ||
    TEAMMATE_MESSAGE.test(text)
  );
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

/** Raw user text from a user_message payload — content (string) else text. */
export function userMessageText(payload: unknown): string {
  const p = asObj(payload);
  if (typeof p.content === 'string') return p.content;
  if (typeof p.text === 'string') return p.text;
  return '';
}

/** Classify the caller of a user_message. Only meaningful for kind
 *  'user_message' — callers gate on kind. `is_meta` accepts the DTO's
 *  `boolean | number` (NULL TEXT row mapping yields 0/1). */
export function messageOrigin(e: {
  payload: unknown;
  is_meta?: boolean | number | null;
}): OriginResult {
  const text = userMessageText(e.payload);
  // Markers are checked before isMeta so a command invocation (often
  // isMeta=false) and a caveat (isMeta=true) both land on their specific origin.
  if (COMMAND_INVOCATION.test(text)) {
    const name = text.match(/<command-name>([^<]*)<\/command-name>/)?.[1]?.trim() || null;
    return { origin: 'command', commandName: name };
  }
  if (COMMAND_OUTPUT.test(text)) return { origin: 'command-output', commandName: null };
  if (SYSTEM_BEAT.test(text)) return { origin: 'system', commandName: null };
  if (NOTIFICATION.test(text)) return { origin: 'notification', commandName: null };
  if (TEAMMATE_MESSAGE.test(text)) {
    const id = text.match(/<teammate-message[^>]*\steammate_id="([^"]*)"/)?.[1] ?? null;
    return { origin: 'teammate', commandName: null, teammateId: id };
  }
  if (SKILL_SCAFFOLD.test(text)) return { origin: 'skill', commandName: null };
  // No marker: isMeta=true is the remaining injection signal (skill bodies,
  // injected guidance). Everything else — including <system-reminder>-wrapped
  // turns — is genuine human input.
  if (e.is_meta) return { origin: 'skill', commandName: null };
  return { origin: 'human', commandName: null };
}

/** Human-readable body for a user record, by origin: a command invocation
 *  collapses its <command-*> scaffolding to "/name args"; command output strips
 *  the <local-command-*> wrapper to its inner text; everything else is shown
 *  verbatim. Keeps the raw XML scaffolding out of the rendered bubble while
 *  still attributing the record to the user who triggered it. */
export function userDisplayText(origin: MessageOrigin, text: string, commandName: string | null): string {
  if (origin === 'command') {
    const args = text.match(/<command-args>([\s\S]*?)<\/command-args>/)?.[1]?.trim();
    return [commandName, args].filter((s) => s && s.length).join(' ').trim() || (commandName ?? text.trim());
  }
  if (origin === 'command-output') {
    const inner = text.match(/<local-command-(?:stdout|caveat)>([\s\S]*?)<\/local-command-(?:stdout|caveat)>/);
    return (inner?.[1] ?? text).trim();
  }
  return text.trim();
}
