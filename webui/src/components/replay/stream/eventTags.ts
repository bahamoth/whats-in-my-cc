// webui/src/components/replay/stream/eventTags.ts
//
// 태그 어휘·분류기는 Rust core(`src/insight/event_tags.rs`)가 소유한다
// (loop-foundations 2026-06-12 — 종전에는 이 파일이 사전·셸 파서를 들고 있어
// MCP 소비자는 raw tool_name밖에 보지 못했다). 서버가 렌더된 tool_call
// 이벤트에 `tag {value, disposition, token, display}`를 실어 주고, 이 모듈은
// 그 값의 표현(칩 verb 색)과 집계(untagged 패널·태깅 루프 CLI)만 담당한다.
//
// 분류 규칙 추가(태깅 루프)는 src/insight/event_tags.rs의 사전에:
// 일반 첫 토큰 → BASH_FIRST_TOKEN_TAGS, 멀티플렉서 서브커맨드 →
// TOOL_SUBCOMMAND_TAGS, Read/Edit 확장자 → EXT_OBJECT. 분류 테스트는
// tests/event_tags.rs — webui에는 분류 로직이 없다. 규칙 변경은 서버 재빌드·
// 재기동 후 반영된다(태그는 API 렌더 시점 계산).
import type { ObservedEventDto } from '../../../api/types';

// ── Taxonomy: every tag is `verb.object` ──────────────────────────────────
//   verbs   : read · write · delete · build · test · run · lint
//   objects : code · docs · config · data · image · file · proc · vcs · db · web · deps
// The chip is coloured by the VERB (the part before the dot).
export type Tag =
  | 'read.code' | 'read.docs' | 'read.config' | 'read.data' | 'read.image'
  | 'read.file' | 'read.proc' | 'read.vcs' | 'read.db' | 'read.web'
  | 'write.file' | 'write.vcs' | 'write.deps'
  | 'write.code' | 'write.docs' | 'write.config' | 'write.data' | 'write.image'
  | 'delete.file'
  | 'build.code' | 'test.code' | 'run.code' | 'lint.code';

/** The verb (action) component of a tag — used for chip colouring/grouping. */
export type TagVerb = 'read' | 'write' | 'delete' | 'build' | 'test' | 'run' | 'lint';
export function tagVerb(tag: Tag): TagVerb {
  return tag.slice(0, tag.indexOf('.')) as TagVerb;
}

export interface UntaggedRow {
  token: string;
  count: number;
  sample: string;
  hint: string;
  /** event_id of the FIRST occurrence — the panel links to this card. */
  eventId: string;
}

const RULES_FILE = 'src/insight/event_tags.rs';
const FILE_TOOLS = new Set(['Read', 'Edit', 'Write', 'MultiEdit']);

/** 태깅 루프가 정확한 사전 위치를 알도록 token 형태·도구별 힌트를 만든다.
 *  사전은 이제 Rust core에 있다 — eventTags.ts에는 규칙이 없다. */
function hintFor(toolName: string | null, token: string): string {
  if (toolName && FILE_TOOLS.has(toolName)) {
    return `add '${token}' to EXT_OBJECT in ${RULES_FILE} (확장자→object 매핑; 확장자 없는 파일은 별도 규칙 검토)`;
  }
  if (token.includes(' ')) {
    const [tool, sub] = token.split(' ');
    return `add '${sub}': '<tag>' to TOOL_SUBCOMMAND_TAGS['${tool}'] in ${RULES_FILE}`;
  }
  return `add '${token}': '<tag>' to BASH_FIRST_TOKEN_TAGS in ${RULES_FILE}`;
}

/** 서버가 분류한 `tag` 필드 기준으로 unmatched 이벤트를 token별 집계한다.
 *  (untagged 패널과 scripts/untagged-bash.ts의 SSOT — 분류 자체는 서버 몫.) */
export function collectUntagged(events: ObservedEventDto[]): UntaggedRow[] {
  const byToken = new Map<string, { count: number; sample: string; eventId: string; hint: string }>();
  for (const e of events) {
    const t = e.tag;
    if (!t || t.disposition !== 'unmatched') continue;
    const token = t.token ?? '';
    if (!token) continue;
    const cur = byToken.get(token);
    if (cur) cur.count++;
    else
      byToken.set(token, {
        count: 1,
        sample: (t.display ?? '').slice(0, 80),
        eventId: e.event_id,
        hint: hintFor(e.tool_name, token),
      });
  }
  return [...byToken.entries()]
    .map(([token, v]) => ({ token, ...v }))
    .sort((a, b) => b.count - a.count);
}
