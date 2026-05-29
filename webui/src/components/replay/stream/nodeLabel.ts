export interface NodeLabel {
  kind: 'user' | 'assistant' | 'thinking' | 'tool' | 'hook' | 'span' | 'verify' | 'diff' | 'other';
  primary: string;
  secondary: string;
}

const SCAFFOLD =
  /^\s*(<command-name>|<command-message>|<command-args>|<local-command-stdout>|<local-command-caveat>|Base directory for this skill:|\[Request interrupted)/;

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

function toolArg(input: unknown): string {
  const i = asObj(input);
  for (const k of ['command', 'file_path', 'pattern', 'skill', 'path', 'query', 'url']) {
    if (typeof i[k] === 'string') {
      const val = i[k] as string;
      if (k === 'file_path' || k === 'path') {
        return val.split('/').pop() ?? val;
      }
      return val;
    }
  }
  return '';
}

export function nodeLabel(node: { node_kind: string; payload: unknown }): NodeLabel {
  const p = asObj(node.payload);
  switch (node.node_kind) {
    case 'tool_call':
      return {
        kind: 'tool',
        primary: (p.tool_name as string) || 'tool',
        secondary: toolArg(p.input),
      };
    case 'assistant_message':
      return {
        kind: 'assistant',
        primary: formatModel(p.model),
        secondary: ((p.text as string) ?? '').trim(),
      };
    case 'thinking':
      return {
        kind: 'thinking',
        primary: '추론',
        secondary: ((p.thinking as string) ?? '').trim(),
      };
    case 'user_message': {
      const txt =
        typeof p.content === 'string' ? p.content : ((p.text as string) ?? '');
      if (SCAFFOLD.test(txt)) {
        const name =
          txt.match(/<command-name>([^<]*)<\/command-name>/)?.[1] ?? 'scaffolding';
        return { kind: 'user', primary: 'command', secondary: name };
      }
      return { kind: 'user', primary: 'You', secondary: txt.trim() };
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
        secondary: (asObj(p.raw_span).name as string) ?? '',
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
    default:
      return { kind: 'other', primary: node.node_kind, secondary: '' };
  }
}
