// webui/src/components/replay/stream/MessageCard.tsx
import { useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { User, Bot, BrainCog, Lightbulb, CornerDownRight, Info, Code2, Type } from 'lucide-react';
import type { MessageItem } from './streamModel';
import { formatModel } from './nodeLabel';
import styles from './MessageCard.module.css';

function timeLabel(iso: string): string {
  // HH:MM:SS on the viewer's local clock; fall back to the raw string.
  // (Was toISOString().slice(...) — that is UTC, contradicting this intent.)
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

// Long-message clamp trigger. Character/line heuristics (not measured pixels)
// so the decision is deterministic and testable; the visual cut itself is CSS
// max-height. Roughly two screens of chat text.
const CLAMP_CHARS = 1500;
const CLAMP_LINES = 24;

function isClampable(text: string): boolean {
  if (text.length > CLAMP_CHARS) return true;
  let lines = 1;
  for (let i = 0; i < text.length; i++) if (text[i] === '\n') lines++;
  return lines > CLAMP_LINES;
}

interface MessageCardProps {
  item: MessageItem;
  selected: boolean;
  onSelect: (eventId: string) => void;
  hasFinding?: boolean;
}

export function MessageCard({ item, selected, onSelect, hasFinding = false }: MessageCardProps) {
  // Markdown view mode. Assistant/system output is authored AS markdown →
  // styled by default; user prompts and thinking are literal text where `_`/`*`
  // are usually paths or emphasis-by-accident → raw by default. The per-card
  // toggle flips either way (원본 보기 ↔ 스타일 보기).
  const defaultStyled = item.role === 'assistant' || item.role === 'system';
  const [styledOverride, setStyledOverride] = useState<boolean | null>(null);
  const styled = styledOverride ?? defaultStyled;

  // Long messages (huge prompts, pasted logs) collapse to a fixed height with
  // a 더 보기/접기 control so one card cannot swallow the whole stream.
  const clampable = isClampable(item.text);
  const [clampOpen, setClampOpen] = useState(false);
  const clamped = clampable && !clampOpen;

  // A sidechain user_message is the orchestrator's prompt to a Task subagent —
  // not human input. It renders left, labelled "Prompt", inside a SubagentGroup.
  const isSubagentPrompt = item.role === 'user' && item.sidechain;
  const isRight = item.role === 'user' && !item.sidechain;
  const align = isRight ? 'right' : 'left';

  let Icon: typeof User;
  let label: string;
  let bubbleClass: string;

  // Origin chip — surfaces who produced this message (the raw role the user
  // would otherwise have to open Raw to confirm): a human, a subagent, or the
  // model. Complements the friendly label.
  const sourceTag = isSubagentPrompt
    ? 'subagent'
    : item.role === 'user'
    ? 'external'
    : item.role === 'system'
    ? 'system'
    : 'agent';

  if (isSubagentPrompt) {
    Icon = CornerDownRight;
    label = 'Prompt';
    bubbleClass = styles.subagentBubble;
  } else if (item.role === 'user') {
    Icon = User;
    label = 'You';
    bubbleClass = styles.userBubble;
  } else if (item.role === 'assistant') {
    Icon = Bot;
    label = formatModel(item.model);
    bubbleClass = styles.assistantBubble;
  } else if (item.role === 'system') {
    // system_summary — a CC work recap (away_summary) or a thinner status beat.
    Icon = Info;
    label = '요약';
    bubbleClass = styles.systemBubble;
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
        <span data-testid="source-badge" className={styles.sourceBadge}>{sourceTag}</span>
        {hasFinding && (
          <Lightbulb
            size={12}
            aria-label="has finding"
            className={styles.finding}
          />
        )}
        <button
          data-testid="md-toggle"
          className={styles.mdToggle}
          aria-label={styled ? '원본 보기' : '마크다운 보기'}
          title={styled ? '원본 보기' : '마크다운 보기'}
          onClick={(e) => {
            e.stopPropagation();
            setStyledOverride(!styled);
          }}
        >
          {styled ? <Code2 size={12} aria-hidden /> : <Type size={12} aria-hidden />}
        </button>
        <span className={styles.time}>{timeLabel(item.timestamp)}</span>
      </div>
      <div
        data-testid="message-bubble"
        data-mode={styled ? 'styled' : 'raw'}
        {...(clampable ? { 'data-clamped': String(clamped) } : {})}
        className={`${styles.bubble} ${bubbleClass} ${styled ? styles.markdown : ''} ${clamped ? styles.clamped : ''}`}
      >
        {styled ? <Markdown remarkPlugins={[remarkGfm]}>{item.text}</Markdown> : item.text}
      </div>
      {clampable && (
        <button
          data-testid="clamp-toggle"
          className={styles.clampToggle}
          onClick={(e) => {
            e.stopPropagation();
            setClampOpen(!clampOpen);
          }}
        >
          {clamped ? '더 보기' : '접기'}
        </button>
      )}
    </div>
  );
}
