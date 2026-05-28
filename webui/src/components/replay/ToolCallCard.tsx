/**
 * PR-5 — tool_call (and optional tool_result) card. Pure presentation;
 * extraction from the underlying DTO is the caller's responsibility.
 * rawPayload is held as a prop *only* to make the "raw" toggle handler
 * trivial — its content NEVER reaches the DOM.
 */
import { formatMs } from '../../lib/format';
import styles from './ToolCallCard.module.css';

export type ToolStatus = 'ok' | 'error' | 'pending';

const MAX_INPUT_CHARS = 120;
const MAX_OUTPUT_LINES = 3;

interface ToolCallCardProps {
  toolName: string;
  status: ToolStatus;
  inputSummary?: string;
  outputPreview?: string;
  latencyMs?: number;
  onOpenRaw?: () => void;
  /** Held internally only — never rendered. Used so the parent can pass
   *  the full payload reference for the raw drawer without managing it
   *  externally. Marked `unknown` so TypeScript users don't mistakenly
   *  read structured fields off this prop. */
  rawPayload?: unknown;
}

function truncateInput(s: string): string {
  if (s.length <= MAX_INPUT_CHARS) return s;
  return s.slice(0, MAX_INPUT_CHARS) + '…';
}

function clipOutput(s: string): string {
  const lines = s.split('\n');
  if (lines.length <= MAX_OUTPUT_LINES) return s;
  return lines.slice(0, MAX_OUTPUT_LINES).join('\n');
}

export function ToolCallCard({
  toolName,
  status,
  inputSummary,
  outputPreview,
  latencyMs,
  onOpenRaw,
  // rawPayload is intentionally not destructured into the render tree.
}: ToolCallCardProps) {
  return (
    <article className={styles.card} data-testid="toolcall-card" data-state={status}>
      <header className={styles.header}>
        <span className={styles.toolName}>{toolName}</span>
        <span className={styles.status} data-state={status}>{status}</span>
        {typeof latencyMs === 'number' && (
          <span className={styles.latency} data-testid="toolcall-latency">
            {formatMs(latencyMs)}
          </span>
        )}
        {onOpenRaw && (
          <button
            type="button"
            className={styles.rawBtn}
            onClick={onOpenRaw}
            aria-label="Open raw record"
          >
            raw
          </button>
        )}
      </header>
      {inputSummary && (
        <div className={styles.input} data-testid="toolcall-input">
          {truncateInput(inputSummary)}
        </div>
      )}
      {outputPreview && (
        <pre className={styles.output} data-testid="toolcall-output">
          {clipOutput(outputPreview)}
        </pre>
      )}
    </article>
  );
}
