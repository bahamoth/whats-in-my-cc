/**
 * PR-5 — chat turn bubble. Decoupled from any specific DTO so the page
 * can adapt either an ObservedEvent.user_message record or a graph
 * node payload. raw payload is intentionally NOT a prop here; the caller
 * triggers BottomDrawer via `onOpenRaw`.
 */
import { formatTokens } from '../../lib/format';
import styles from './ChatMessageNode.module.css';

export type ChatRole = 'user' | 'assistant' | 'system';

interface ChatMessageNodeProps {
  role: ChatRole;
  text: string;
  tokenCount?: number;
  onOpenRaw?: () => void;
}

export function ChatMessageNode({ role, text, tokenCount, onOpenRaw }: ChatMessageNodeProps) {
  return (
    <article className={styles.bubble} data-role={role}>
      <header className={styles.header}>
        <span className={styles.pill} data-testid="chat-role-pill">{role}</span>
        {typeof tokenCount === 'number' && tokenCount > 0 && (
          <span className={styles.chip} data-testid="chat-token-chip">
            {formatTokens(tokenCount)} tok
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
      <p className={styles.text}>{text}</p>
    </article>
  );
}
