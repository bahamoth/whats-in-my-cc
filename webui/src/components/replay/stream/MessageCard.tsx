// webui/src/components/replay/stream/MessageCard.tsx
import { User, Bot, BrainCog, Lightbulb } from 'lucide-react';
import type { MessageItem } from './streamModel';
import { formatModel } from './nodeLabel';
import styles from './MessageCard.module.css';

function timeLabel(iso: string): string {
  // HH:MM:SS in the viewer's locale; fall back to the raw string.
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().slice(11, 19);
}

interface MessageCardProps {
  item: MessageItem;
  selected: boolean;
  onSelect: (eventId: string) => void;
  hasFinding?: boolean;
}

export function MessageCard({ item, selected, onSelect, hasFinding = false }: MessageCardProps) {
  const isRight = item.role === 'user';
  const align = isRight ? 'right' : 'left';

  let Icon: typeof User;
  let label: string;
  let bubbleClass: string;

  if (item.role === 'user') {
    Icon = User;
    label = 'You';
    bubbleClass = styles.userBubble;
  } else if (item.role === 'assistant') {
    Icon = Bot;
    label = formatModel(item.model);
    bubbleClass = styles.assistantBubble;
  } else {
    // thinking
    Icon = BrainCog;
    label = '추론';
    bubbleClass = styles.thinkingBubble;
  }

  return (
    <div
      data-testid="message-card"
      data-role={item.role}
      data-align={align}
      data-selected={String(selected)}
      role="button"
      tabIndex={0}
      className={`${styles.card} ${isRight ? styles.alignRight : styles.alignLeft}`}
      onClick={() => onSelect(item.eventId)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect(item.eventId);
        }
      }}
    >
      <div className={styles.head}>
        <Icon size={14} aria-hidden className={styles.icon} />
        <span className={styles.label}>{label}</span>
        {hasFinding && (
          <Lightbulb
            size={12}
            aria-label="has finding"
            className={styles.finding}
          />
        )}
        <span className={styles.time}>{timeLabel(item.timestamp)}</span>
      </div>
      <div className={`${styles.bubble} ${bubbleClass}`}>
        {item.text}
      </div>
    </div>
  );
}
