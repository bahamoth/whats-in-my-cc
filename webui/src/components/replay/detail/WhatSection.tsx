// webui/src/components/replay/detail/WhatSection.tsx
//
// Per-kind "what it did" body for the WHAT (①) layer in InsightTab.
// Shows full command input + matched tool_result output for tool_call;
// full text for user/assistant; per-type fields for other kinds.
import type { ObservedEventDto } from '../../../api/types';
import styles from './WhatSection.module.css';

// Max chars to show for long outputs before truncating
const OUTPUT_TRUNCATE = 2000;

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function truncate(s: string, max = OUTPUT_TRUNCATE): { text: string; truncated: boolean } {
  if (s.length <= max) return { text: s, truncated: false };
  return { text: s.slice(0, max), truncated: true };
}

interface WhatSectionProps {
  event: ObservedEventDto;
  matchedResult: ObservedEventDto | null;
}

// Bash/tool_call body
function ToolCallBody({ event, matchedResult }: WhatSectionProps) {
  const p = asObj(event.payload);
  const input = asObj(p.input);

  // Build the command/input display — show full value (WhatSection shows full; header shows summary)
  let cmdDisplay = '';
  if (typeof input.command === 'string') {
    cmdDisplay = input.command;
  } else if (typeof input.file_path === 'string') {
    cmdDisplay = input.file_path;
  } else if (typeof input.pattern === 'string') {
    cmdDisplay = input.pattern;
  } else if (typeof input.query === 'string') {
    cmdDisplay = input.query;
  } else if (typeof input.path === 'string') {
    cmdDisplay = input.path;
  } else if (typeof input.action === 'string') {
    // computer/browser action
    const a = input.action as string;
    if (Array.isArray(input.coordinate)) cmdDisplay = `${a} (${(input.coordinate as number[]).join(', ')})`;
    else if (typeof input.text === 'string') cmdDisplay = `${a} "${input.text}"`;
    else if (typeof input.url === 'string') cmdDisplay = `${a} ${input.url}`;
    else cmdDisplay = a;
  } else {
    // fallback: dump input as JSON if small
    const j = JSON.stringify(input, null, 2);
    cmdDisplay = j.length < 500 ? j : j.slice(0, 500) + '\n…';
  }

  // matched tool_result output
  let resultContent: string | null = null;
  let isError = false;
  if (matchedResult) {
    const rp = asObj(matchedResult.payload);
    const tr = asObj(rp.tool_result);
    if (typeof tr.content === 'string') {
      resultContent = tr.content;
    } else if (Array.isArray(tr.content)) {
      // content may be array of {type,text} blocks
      resultContent = (tr.content as Array<{type?: string; text?: string}>)
        .map((b) => (typeof b.text === 'string' ? b.text : JSON.stringify(b)))
        .join('\n');
    }
    isError = Boolean(tr.is_error);
  }

  const { text: cmdText } = truncate(cmdDisplay);
  const result = resultContent ? truncate(resultContent) : null;

  return (
    <div className={styles.what}>
      <div className={styles.cmd}>{cmdText}</div>
      {resultContent !== null && (
        <>
          <div className={`${isError ? styles.err : styles.out} ${result?.truncated ? styles.truncated : ''}`}>
            {isError && <span className={styles.errBadge}>오류</span>}
            <span>{result?.text}</span>
          </div>
          {result?.truncated && (
            <span className={styles.truncNote}>… {OUTPUT_TRUNCATE.toLocaleString()}자 이후 잘림 — Raw 탭에서 전문</span>
          )}
        </>
      )}
    </div>
  );
}

export function WhatSection({ event, matchedResult }: WhatSectionProps) {
  const p = asObj(event.payload);

  switch (event.kind) {
    case 'tool_call':
      return <ToolCallBody event={event} matchedResult={matchedResult} />;

    case 'user_message': {
      const content = typeof p.content === 'string' ? p.content : (typeof p.text === 'string' ? p.text : '');
      const { text, truncated } = truncate(content);
      return (
        <div className={styles.what}>
          <div className={`${styles.prose} ${truncated ? styles.truncated : ''}`}>{text}</div>
          {truncated && (
            <span className={styles.truncNote}>… {OUTPUT_TRUNCATE.toLocaleString()}자 이후 잘림 — Raw 탭에서 전문</span>
          )}
        </div>
      );
    }

    case 'assistant_message': {
      const text_ = typeof p.text === 'string' ? p.text : '';
      const { text, truncated } = truncate(text_);
      return (
        <div className={styles.what}>
          <div className={`${styles.prose} ${truncated ? styles.truncated : ''}`}>{text}</div>
          {truncated && (
            <span className={styles.truncNote}>… {OUTPUT_TRUNCATE.toLocaleString()}자 이후 잘림 — Raw 탭에서 전문</span>
          )}
        </div>
      );
    }

    case 'thinking':
      return (
        <div className={styles.what}>
          <div className={styles.notice}>추론 본문은 기록되지 않음 (signature only)</div>
        </div>
      );

    case 'hook_event': {
      const hookName =
        (p.hookName as string) ??
        (asObj(p.hook).hook_event_name as string) ??
        '';
      const cmd = (p.command as string) ?? '';
      const stdout = (p.stdout as string) ?? '';
      const stderr = (p.stderr as string) ?? '';
      return (
        <div className={styles.what}>
          {hookName && <div className={styles.cmd}>{hookName}</div>}
          {cmd && <div className={styles.cmd}>{cmd}</div>}
          {stdout && <div className={styles.out}>{truncate(stdout).text}</div>}
          {stderr && <div className={styles.err}>{truncate(stderr).text}</div>}
        </div>
      );
    }

    case 'diff_hunk': {
      const patch = (p.patch_preview as string) ?? (p.patch as string) ?? '';
      const filePath = (p.file_path as string) ?? (p.path as string) ?? '';
      return (
        <div className={styles.what}>
          {filePath && <div className={styles.prose}>{filePath}</div>}
          {patch && <div className={styles.cmd}>{truncate(patch).text}</div>}
        </div>
      );
    }

    case 'verification_run': {
      const cmd = (p.command as string) ?? '';
      const status = (p.status as string) ?? '';
      const failSummary = (p.failure_summary as string) ?? '';
      return (
        <div className={styles.what}>
          {cmd && <div className={styles.cmd}>{cmd}</div>}
          {status && <div className={styles.prose}>{status}</div>}
          {failSummary && <div className={styles.err}>{failSummary}</div>}
        </div>
      );
    }

    default: {
      // For unrecognised kinds (otel_span, log_record, etc.), show key fields
      // if there are a handful, otherwise defer to the Raw tab.
      const entries = Object.entries(p).filter(
        ([, v]) => typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean',
      ).slice(0, 5);
      if (entries.length > 0) {
        return (
          <div className={styles.what}>
            {entries.map(([k, v]) => (
              <div key={k} className={styles.prose}>
                <span className={styles.key}>{k}:</span> {String(v)}
              </div>
            ))}
          </div>
        );
      }
      return (
        <div className={styles.what}>
          <div className={styles.notice}>원본은 Raw 탭 참조</div>
        </div>
      );
    }
  }
}
