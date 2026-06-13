// webui/src/components/replay/stream/MessageCard.tsx
import { useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { User, Bot, BrainCog, Lightbulb, CornerDownRight, Info, Code2, Type, Terminal, Sparkles, Bell } from 'lucide-react';
import type { MessageItem } from './streamModel';
import { userDisplayText } from './messageOrigin';
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
  // A sidechain user_message is the orchestrator's prompt to a Task subagent —
  // not human input. It renders left, labelled "Prompt", inside a SubagentGroup.
  const isSubagentPrompt = item.role === 'user' && item.sidechain;
  const isRight = item.role === 'user' && !item.sidechain;
  const align = isRight ? 'right' : 'left';

  // Caller classification only applies to a non-sidechain user record. CC folds
  // typed input + invoked command/skill scaffolding + command output into
  // type:"user" — all user-ORIGINATED, so all stay on the user side; the origin
  // only changes the label/chip/icon and whether the injected body collapses, so
  // it never masquerades as the words the user actually typed.
  const userOrigin = (isRight ? item.origin : 'human') ?? 'human';
  const isScaffold = userOrigin !== 'human';

  // Rendered body: strip <command-*>/<local-command-*> scaffolding to a clean
  // line for command/output records; verbatim otherwise.
  const displayText = isRight
    ? userDisplayText(userOrigin, item.text, item.commandName ?? null)
    : item.text;

  // Markdown view mode. Assistant/system output is authored AS markdown →
  // styled by default; user prompts, scaffolding and thinking are literal text
  // where `_`/`*` are usually paths or emphasis-by-accident → raw by default.
  const defaultStyled = item.role === 'assistant' || item.role === 'system';
  const [styledOverride, setStyledOverride] = useState<boolean | null>(null);
  const styled = styledOverride ?? defaultStyled;

  // Long messages (huge prompts, pasted logs) collapse to a fixed height with a
  // 더 보기/접기 control so one card cannot swallow the whole stream. Injected
  // skill bodies / command output collapse BY DEFAULT regardless of length —
  // they are reference, not conversation.
  const forceCollapse = userOrigin === 'skill' || userOrigin === 'command-output';
  const clampable = forceCollapse || isClampable(displayText);
  const [clampOpen, setClampOpen] = useState(false);
  const clamped = clampable && !clampOpen;

  let Icon: typeof User;
  let label: string;
  let bubbleClass: string;

  // Origin chip — surfaces who produced this record: typed by a human, a
  // command/skill the user invoked, command output, a subagent prompt, or the
  // model. Replaces the old hardcoded "external" (a string unrelated to the
  // transcript's userType).
  const sourceTag = isSubagentPrompt
    ? 'subagent'
    : item.role === 'user'
    ? userOrigin === 'human'
      ? 'human'
      : userOrigin === 'command-output'
      ? 'command'
      : userOrigin // 'command' | 'skill' | 'system' | 'notification'
    : item.role === 'system'
    ? 'system'
    : 'agent';

  if (isSubagentPrompt) {
    Icon = CornerDownRight;
    label = 'Prompt';
    bubbleClass = styles.subagentBubble;
  } else if (item.role === 'user' && isScaffold) {
    // user-invoked command / injected skill body / command output / interrupt /
    // harness background-task notification.
    Icon =
      userOrigin === 'skill'
        ? Sparkles
        : userOrigin === 'system'
        ? Info
        : userOrigin === 'notification'
        ? Bell
        : Terminal;
    label =
      userOrigin === 'command'
        ? (item.commandName ?? 'Command')
        : userOrigin === 'skill'
        ? 'Skill'
        : userOrigin === 'command-output'
        ? 'Command output'
        : userOrigin === 'notification'
        ? '알림'
        : 'System';
    bubbleClass = styles.metaBubble;
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
      {...(isRight ? { 'data-origin': userOrigin } : {})}
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
        {styled ? <Markdown remarkPlugins={[remarkGfm]}>{displayText}</Markdown> : displayText}
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
