// webui/src/components/replay/stream/__tests__/ScaffoldGroup.test.tsx
// ScaffoldGroup folds a contiguous run of user-side scaffold messages (commands,
// skill bodies, command output, interrupts, task-notifications) into a single
// collapsible block, mirroring the SubagentGroup/BatchGroup fold pattern.
//  - Collapsed (default): violet "커맨드·스킬" chip + count + preview
//    (commandNames + a hint for the rest); the individual cards stay hidden.
//  - Expanded: each MessageCard renders.
//  - Selection inside the group auto-expands it.
// Anchored to session 5bde98d8's 6-card scaffold run (design 2026-06-14 §1).
import { screen, fireEvent } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import { describe, it, expect, vi } from 'vitest';
import { ScaffoldGroup } from '../ScaffoldGroup';
import type { MessageItem, ScaffoldGroup as ScaffoldGroupModel } from '../streamModel';

function smsg(id: string, over: Partial<MessageItem> = {}): MessageItem {
  return {
    type: 'message',
    id,
    eventId: id,
    role: 'user',
    model: null,
    text: id,
    timestamp: '2026-06-13T00:00:00Z',
    sidechain: false,
    origin: 'command',
    commandName: null,
    ...over,
  };
}

function group(over: Partial<ScaffoldGroupModel> = {}): ScaffoldGroupModel {
  return {
    type: 'scaffold-group',
    id: 'scaffold-c1',
    items: [
      smsg('s1', { origin: 'system', text: '[Request interrupted by user]' }),
      smsg('c1', { origin: 'command', commandName: '/chrome', text: '<command-name>/chrome</command-name>' }),
      smsg('o1', { origin: 'command-output', text: '<local-command-stdout>out</local-command-stdout>' }),
      smsg('c2', { origin: 'command', commandName: '/claude-in-chrome', text: '<command-name>/claude-in-chrome</command-name>' }),
      smsg('k1', { origin: 'skill', text: 'Base directory for this skill: /x' }),
    ],
    commandNames: ['/chrome', '/claude-in-chrome'],
    ...over,
  };
}

describe('ScaffoldGroup', () => {
  it('collapsed by default: shows the "커맨드·스킬" chip, count, and preview; hides the cards', () => {
    render(
      <ScaffoldGroup
        group={group()}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    const root = screen.getByTestId('scaffold-group');
    expect(root).toHaveAttribute('data-expanded', 'false');
    expect(screen.getByText('커맨드·스킬')).toBeInTheDocument();
    // count of folded items
    expect(root).toHaveTextContent('5');
    // preview surfaces the invoked command names
    const preview = screen.getByTestId('scaffold-preview');
    expect(preview).toHaveTextContent('/chrome');
    expect(preview).toHaveTextContent('/claude-in-chrome');
    // collapsed → individual cards are not mounted
    expect(screen.queryByTestId('message-card')).toBeNull();
  });

  it('preview hints at the non-command remainder (system/output/skill) beyond the named commands', () => {
    render(
      <ScaffoldGroup
        group={group()}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    // 5 items, 2 are named commands → 3 others surfaced as a "+N" hint.
    expect(screen.getByTestId('scaffold-preview')).toHaveTextContent('3');
  });

  it('expands on toggle: renders one MessageCard per folded item', () => {
    render(
      <ScaffoldGroup
        group={group()}
        selectedEventId={null}
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    fireEvent.click(screen.getByTestId('scaffold-toggle'));
    expect(screen.getByTestId('scaffold-group')).toHaveAttribute('data-expanded', 'true');
    expect(screen.getAllByTestId('message-card')).toHaveLength(5);
  });

  it('auto-expands when the selected event lives inside the group', () => {
    render(
      <ScaffoldGroup
        group={group()}
        selectedEventId="c2"
        onSelect={() => {}}
        findingEventIds={new Set()}
      />,
    );
    expect(screen.getByTestId('scaffold-group')).toHaveAttribute('data-expanded', 'true');
    expect(screen.getAllByTestId('message-card').length).toBe(5);
  });

  it('forwards onSelect from a child card to the host', () => {
    const onSelect = vi.fn();
    render(
      <ScaffoldGroup
        group={group()}
        selectedEventId="c2"
        onSelect={onSelect}
        findingEventIds={new Set()}
      />,
    );
    // expanded (selection inside) → cards clickable
    screen.getAllByTestId('message-card')[0].click();
    expect(onSelect).toHaveBeenCalledWith('s1');
  });

  it('a group with no named commands still previews a sensible hint', () => {
    const g = group({
      items: [
        smsg('s1', { origin: 'system' }),
        smsg('o1', { origin: 'command-output' }),
      ],
      commandNames: [],
    });
    render(
      <ScaffoldGroup group={g} selectedEventId={null} onSelect={() => {}} findingEventIds={new Set()} />,
    );
    expect(screen.getByTestId('scaffold-group')).toHaveAttribute('data-expanded', 'false');
    // count still shows
    expect(screen.getByTestId('scaffold-group')).toHaveTextContent('2');
  });
});
