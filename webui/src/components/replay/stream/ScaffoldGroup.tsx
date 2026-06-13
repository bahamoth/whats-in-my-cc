// webui/src/components/replay/stream/ScaffoldGroup.tsx
// Renders a contiguous run of user-side scaffold messages (slash-command
// invocations, injected skill bodies, command output, system interrupts,
// harness task-notifications) as ONE collapsible block. CC folds these into
// type:"user" records the user TRIGGERED, so they stay on the user side, but a
// run of them buries the real conversation — collapsing them keeps the human
// input + assistant flow legible. Collapsed (the default, since scaffold is
// "reference, not conversation") it reads as a violet "커맨드·스킬" chip + a
// count + a preview (the invoked command names plus a hint for the rest);
// expanded it shows the individual MessageCards verbatim.
//
// Same fold policy + prop signature as SubagentGroup/BatchGroup so
// ConversationStream swaps it into renderItem the same way.
import { useState } from 'react';
import { ChevronDown, ChevronRight, Terminal } from 'lucide-react';
import { MessageCard } from './MessageCard';
import type { ScaffoldGroup as ScaffoldGroupModel } from './streamModel';
import styles from './ScaffoldGroup.module.css';

interface ScaffoldGroupProps {
  group: ScaffoldGroupModel;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  findingEventIds: Set<string>;
}

export function ScaffoldGroup({
  group,
  selectedEventId,
  onSelect,
  findingEventIds,
}: ScaffoldGroupProps) {
  // Fold policy mirrors SubagentGroup: null = no explicit choice yet → follow
  // containsSelected (auto-open when a selection lands inside so the host can
  // scroll it into view); an explicit toggle then wins. Scaffold is reference,
  // not conversation, so the resting default is COLLAPSED (containsSelected is
  // false → false).
  const [userOverride, setUserOverride] = useState<boolean | null>(null);
  const containsSelected =
    selectedEventId != null && group.items.some((it) => it.eventId === selectedEventId);
  const expanded = userOverride ?? containsSelected ?? false;

  const count = group.items.length;
  // The collapsed preview: the invoked command names, then a "+출처 N" hint for
  // the remaining scaffold records (skill bodies / command output / interrupts /
  // notifications) that carry no command name.
  const namedCommands = group.commandNames.join(' ');
  const remainder = count - group.commandNames.length;

  return (
    <section
      data-testid="scaffold-group"
      data-expanded={String(expanded)}
      className={styles.group}
    >
      <div className={styles.headerRow}>
        <button
          data-testid="scaffold-toggle"
          className={styles.header}
          onClick={() => setUserOverride(!expanded)}
          aria-expanded={expanded}
        >
          {expanded ? (
            <ChevronDown size={13} aria-hidden className={styles.chevron} />
          ) : (
            <ChevronRight size={13} aria-hidden className={styles.chevron} />
          )}
          <Terminal size={13} aria-hidden className={styles.icon} />
          <span data-testid="scaffold-chip" className={styles.chip}>
            커맨드·스킬
          </span>
          <span data-testid="scaffold-count" className={styles.count}>
            {count}
          </span>
          <span data-testid="scaffold-preview" className={styles.preview}>
            {namedCommands}
            {remainder > 0 && (
              <span className={styles.hint}>
                {namedCommands ? ' ' : ''}
                +출처 {remainder}
              </span>
            )}
          </span>
        </button>
      </div>
      {expanded && (
        <div className={styles.body}>
          {group.items.map((it) => (
            <MessageCard
              key={it.id}
              item={it}
              selected={it.eventId === selectedEventId}
              onSelect={onSelect}
              hasFinding={findingEventIds.has(it.eventId)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
