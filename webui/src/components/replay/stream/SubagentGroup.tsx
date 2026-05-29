// webui/src/components/replay/stream/SubagentGroup.tsx
// Renders one Task-subagent exchange (a sidechain run) as a single indented
// block: a "Subagent" header + the inner stream (dispatched prompt, the
// subagent's replies, its tool activity) so it reads as separate from — but
// nested under — the main human↔agent conversation.
import { Fragment } from 'react';
import { CornerDownRight } from 'lucide-react';
import { MessageCard } from './MessageCard';
import { ActivityStack } from './ActivityStack';
import { splitRunByPhase } from './activityGroup';
import type { SidechainGroup } from './streamModel';
import styles from './SubagentGroup.module.css';

interface SubagentGroupProps {
  group: SidechainGroup;
  selectedEventId: string | null;
  onSelect: (eventId: string) => void;
  findingEventIds: Set<string>;
  phaseOf: (eventId: string) => string | null;
}

export function SubagentGroup({
  group,
  selectedEventId,
  onSelect,
  findingEventIds,
  phaseOf,
}: SubagentGroupProps) {
  return (
    <section data-testid="subagent-group" className={styles.group}>
      <div className={styles.header}>
        <CornerDownRight size={13} aria-hidden className={styles.icon} />
        <span className={styles.label}>Subagent</span>
      </div>
      <div className={styles.body}>
        {group.items.map((it) => {
          if (it.type === 'message') {
            return (
              <MessageCard
                key={it.id}
                item={it}
                selected={it.eventId === selectedEventId}
                onSelect={onSelect}
                hasFinding={findingEventIds.has(it.eventId)}
              />
            );
          }
          if (it.type === 'activity-run') {
            const stacks = splitRunByPhase(it.events, phaseOf);
            return (
              <Fragment key={it.id}>
                {stacks.map((stack, i) => (
                  <ActivityStack
                    key={`${it.id}-${i}`}
                    stack={stack}
                    selectedEventId={selectedEventId}
                    onSelect={onSelect}
                  />
                ))}
              </Fragment>
            );
          }
          // nested sidechain groups do not occur (grouping is one level deep)
          return null;
        })}
      </div>
    </section>
  );
}
