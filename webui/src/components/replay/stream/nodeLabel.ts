import { messageOrigin } from './messageOrigin';
import type { TFunction } from '../../../i18n';

export interface NodeLabel {
  kind: 'user' | 'assistant' | 'thinking' | 'tool' | 'hook' | 'span' | 'verify' | 'diff' | 'other';
  primary: string;
  secondary: string;
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

export function formatModel(raw: unknown): string {
  if (typeof raw !== 'string' || !raw.startsWith('claude-')) return 'Claude';
  const m = raw.match(/^claude-(opus|sonnet|haiku)-(\d+)-(\d+)/);
  if (!m) return 'Claude';
  const fam = m[1][0].toUpperCase() + m[1].slice(1);
  return `${fam} ${m[2]}.${m[3]}`;
}

// A short "what this tool call did" summary from its input. Beyond the obvious
// single-value keys (command / file_path / …), it handles action-style tools
// (browser/computer: `action` + coordinate/text/url) and falls back to the
// first scalar field so MCP and unknown tools still show *something* instead of
// a bare tool name. The view truncates with ellipsis, so no length cap here.
function toolArg(input: unknown, commandDisplay?: string): string {
  const i = asObj(input);

  // Prefer the tool's own human-readable `description` when present (Bash and
  // Task accept one) — it states the INTENT better than a raw command/args,
  // e.g. "Add costUsd:null to the 5 fixtures via perl" beats the perl one-liner.
  if (typeof i.description === 'string' && i.description.trim()) return i.description.trim();

  // action-style tools (e.g. mcp__claude-in-chrome__computer): "action (x, y)"
  // / 'action "text"' / "action url".
  if (typeof i.action === 'string') {
    const a = i.action;
    if (Array.isArray(i.coordinate)) return `${a} (${i.coordinate.join(', ')})`;
    if (typeof i.text === 'string' && i.text) return `${a} "${i.text}"`;
    if (typeof i.url === 'string' && i.url) return `${a} ${i.url}`;
    return a;
  }

  // common single-value argument keys, in priority order.
  for (const k of [
    'command', 'file_path', 'pattern', 'query', 'url', 'path', 'skill',
    'prompt', 'subagent_type', 'name',
  ]) {
    const val = i[k];
    if (typeof val === 'string' && val) {
      // 서버 tag.display가 선행 `cd …`를 제거한 명령을 준다 (core 분류기) —
      // 없으면(과거 응답·태그 미계산) 원문 그대로.
      if (k === 'command') return commandDisplay ?? val;
      return k === 'file_path' || k === 'path' ? (val.split('/').pop() ?? val) : val;
    }
  }

  // fallback: the first scalar string field, labelled, so unknown/MCP tools
  // still convey what they operated on.
  for (const [k, v] of Object.entries(i)) {
    if (typeof v === 'string' && v) return `${k}: ${v}`;
  }
  return '';
}

/** Parse `mcp__[plugin_<plugin>_]<server>__<tool>` → { server, tool }, or null
 *  for non-MCP names. server = the last underscore-segment of the server-id
 *  (matches the Rust tagger: `plugin_serena_serena` → `serena`,
 *  `claude_ai_Slack` → `Slack`, `claude-in-chrome` → unchanged). */
function parseMcpName(name: string): { server: string; tool: string } | null {
  if (!name.startsWith('mcp__')) return null;
  const rest = name.slice('mcp__'.length);
  const idx = rest.indexOf('__');
  if (idx <= 0) return null;
  const serverId = rest.slice(0, idx);
  const tool = rest.slice(idx + 2);
  if (!serverId || !tool) return null;
  const server = serverId.includes('_') ? serverId.slice(serverId.lastIndexOf('_') + 1) : serverId;
  return { server, tool };
}

/** `mcp__…` → `<server> · <tool>` for display; null for non-MCP names. */
export function formatMcpToolName(name: string): string | null {
  const p = parseMcpName(name);
  return p ? `${p.server} · ${p.tool}` : null;
}

/** The MCP server a tool call belongs to (for matching against the plugin
 *  registry's `mcp_servers`); null for non-MCP names. */
export function mcpServerOf(name: string): string | null {
  return parseMcpName(name)?.server ?? null;
}

/** True when the MCP tool belongs to an official Anthropic integration — a
 *  claude.ai connector (`mcp__claude_ai_<Service>__…`) or the Chrome extension
 *  (`mcp__claude-in-chrome__…`). These are managed, known-semantics servers (not
 *  one-off user configs), so they are tagged and labelled "connector" rather
 *  than "configured" even though they are not marketplace plugins. */
export function mcpOfficialIntegration(name: string): boolean {
  if (!name.startsWith('mcp__')) return false;
  const rest = name.slice('mcp__'.length);
  const idx = rest.indexOf('__');
  if (idx <= 0) return false;
  const serverId = rest.slice(0, idx);
  return serverId.startsWith('claude_ai_') || serverId === 'claude-in-chrome';
}

export function nodeLabel(
  node: {
    node_kind: string;
    payload: unknown;
    // C4 (Tier 3-1): span name lives in the telemetry facet (a sibling of
    // payload), no longer re-embedded under payload.raw_span.
    telemetry?: unknown;
    /** 서버 분류 태그 (tool_call 한정) — display가 명령 표시에 쓰인다. */
    tag?: { display?: string | null } | null;
    /** transcript isMeta → caller classification of user_message scaffolding. */
    is_meta?: boolean | number | null;
  },
  // l10n — the three localized display labels (thinking / command-output /
  // notification) come from the catalog; the caller injects t().
  t: TFunction,
): NodeLabel {
  const p = asObj(node.payload);
  switch (node.node_kind) {
    case 'tool_call': {
      const name = (p.tool_name as string) || 'tool';
      return {
        kind: 'tool',
        // MCP tools render as "server · tool" instead of the raw mcp__… string.
        primary: formatMcpToolName(name) ?? name,
        secondary: toolArg(
          p.input,
          typeof node.tag?.display === 'string' ? node.tag.display : undefined,
        ),
      };
    }
    case 'assistant_message':
      return {
        kind: 'assistant',
        primary: formatModel(p.model),
        secondary: ((p.text as string) ?? '').trim(),
      };
    case 'thinking':
      return {
        kind: 'thinking',
        primary: t('stream.reasoning'),
        secondary: ((p.thinking as string) ?? '').trim(),
      };
    case 'user_message': {
      const txt =
        typeof p.content === 'string' ? p.content : ((p.text as string) ?? '');
      // Shared caller classification (SSOT): a user_message that reaches the
      // activity stack is scaffolding, not human words — label it by origin so
      // it never reads as "You". (Genuine human input is a message bubble and
      // does not come through here.)
      const { origin, commandName } = messageOrigin({ payload: node.payload, is_meta: node.is_meta });
      switch (origin) {
        case 'command':
          return { kind: 'user', primary: 'command', secondary: commandName ?? 'scaffolding' };
        case 'command-output':
          return { kind: 'user', primary: 'command', secondary: t('stream.output') };
        case 'system':
          return { kind: 'user', primary: 'system', secondary: 'interrupted' };
        case 'skill':
          return { kind: 'user', primary: 'skill', secondary: txt.trim().split('\n', 1)[0] };
        case 'notification':
          // Harness background-task completion notice folded into type:"user"
          // (anchored: 전 DB 55건 <task-notification> 선행, isMeta 없음). Label
          // "알림" to match the MessageCard, so the detail panel/activity stack
          // never read it as the user's own "You" input.
          return { kind: 'user', primary: t('stream.notification'), secondary: txt.trim() };
        default:
          return { kind: 'user', primary: 'You', secondary: txt.trim() };
      }
    }
    case 'hook_event': {
      const hn =
        (p.hookName as string) ??
        (asObj(p.hook).hook_event_name as string) ??
        '';
      return { kind: 'hook', primary: 'hook', secondary: hn };
    }
    case 'otel_span':
      return {
        kind: 'span',
        primary: 'span',
        secondary: (asObj(node.telemetry).span_name as string) ?? '',
      };
    case 'verification_run':
      return {
        kind: 'verify',
        primary: 'verify',
        secondary: (p.summary as string) ?? '',
      };
    case 'diff_hunk':
      return {
        kind: 'diff',
        primary: 'diff',
        secondary: (p.file_path as string) ?? (p.path as string) ?? '',
      };
    case 'log_record': {
      // State-change log beats (the STREAM_STATE_LOG whitelist) render here.
      // Friendly name per event_name + the single most salient attribute, so a
      // beat reads e.g. "subagent · Explore" / "mcp · connected", never a bare
      // "log_record". Unknown names fall back to the raw event_name.
      const name = typeof p.event_name === 'string' ? p.event_name : 'log';
      const a = asObj(p.attributes);
      const FRIENDLY: Record<string, string> = {
        subagent_completed: 'subagent',
        mcp_server_connection: 'mcp',
        permission_mode_changed: 'permission mode',
        skill_activated: 'skill',
        compaction: 'compaction',
        at_mention: '@mention',
        feedback_survey: 'survey',
      };
      const detail =
        a.agent_type ?? a.status ?? a.mention_type ?? a.permission_mode ?? a.skill_name ?? '';
      return {
        kind: 'other',
        primary: FRIENDLY[name] ?? name,
        secondary: typeof detail === 'string' ? detail : '',
      };
    }
    default:
      return { kind: 'other', primary: node.node_kind, secondary: '' };
  }
}
