// webui/src/components/replay/stream/StreamCard.tsx
import { User, Bot, BrainCog, Wrench, Link2 } from 'lucide-react';
import type { StreamCard as Card, StreamCardKind } from './streamModel';
import styles from './StreamCard.module.css';

const KIND_META: Record<StreamCardKind, { label: string; Icon: typeof User }> = {
  user: { label: 'User', Icon: User },
  assistant: { label: 'Assistant', Icon: Bot },
  thinking: { label: 'Thinking', Icon: BrainCog },
  tool: { label: 'Tool', Icon: Wrench },
};

function timeLabel(iso: string): string {
  // HH:MM:SS in the viewer's locale; fall back to the raw string.
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().slice(11, 19);
}

interface StreamCardProps {
  card: Card;
  selected: boolean;
  episodePhase: string | null;
  hasFinding: boolean;
  onSelect: (eventId: string) => void;
}

export function StreamCard({ card, selected, episodePhase, hasFinding, onSelect }: StreamCardProps) {
  const meta = KIND_META[card.kind];
  const Icon = meta.Icon;
  return (
    <div
      data-testid="stream-card"
      data-kind={card.kind}
      data-selected={selected ? 'true' : 'false'}
      role="button"
      tabIndex={0}
      className={`${styles.card} ${styles[card.kind]} ${selected ? styles.selected : ''}`}
      onClick={() => onSelect(card.eventId)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect(card.eventId);
        }
      }}
    >
      <div className={styles.head}>
        <span className={styles.kind}>
          <Icon size={14} aria-hidden className={styles.icon} />
          {meta.label}
        </span>
        <span className={styles.time}>{timeLabel(card.timestamp)}</span>
        {episodePhase && <span className={styles.phase} data-phase={episodePhase}>{episodePhase}</span>}
        {hasFinding && <Link2 size={13} aria-label="linked finding" className={styles.finding} />}
      </div>

      {card.kind === 'tool' && card.tool ? (
        <div className={styles.toolBody}>
          <span className={styles.toolName}>{card.tool.toolName}</span>
          {card.tool.inputSummary && <code className={styles.toolArg}>{card.tool.inputSummary}</code>}
          {card.tool.result && (
            <span className={card.tool.result.isError ? styles.badgeError : styles.badgeOk}>
              {card.tool.result.isError ? 'error' : 'ok'}
            </span>
          )}
        </div>
      ) : (
        <p className={styles.preview}>{card.preview}</p>
      )}
    </div>
  );
}
