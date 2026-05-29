// webui/src/components/replay/stream/__tests__/StreamCard.test.tsx
/**
 * R2 RED — StreamCard renders one chat card with actor icon, time, preview,
 * tool summary, error badge, finding marker, and episode chip. Spec §3.2.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { StreamCard } from '../StreamCard';
import type { StreamCard as Card } from '../streamModel';

function card(p: Partial<Card>): Card {
  return { id: 'c', eventId: 'c', kind: 'user', actor: 'user', timestamp: '2026-05-28T09:14:02Z', preview: 'hello', tool: null, ...p };
}

describe('StreamCard', () => {
  it('renders the preview text and a kind label for a user card', () => {
    render(<StreamCard card={card({ kind: 'user', preview: 'fix it' })} selected={false} episodePhase={null} hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText('fix it')).toBeInTheDocument();
    expect(screen.getByText(/user/i)).toBeInTheDocument();
  });

  it('shows tool name and input summary for a tool card', () => {
    render(<StreamCard card={card({ kind: 'tool', preview: '', tool: { toolName: 'Bash', toolUseId: 't', inputSummary: 'cargo test', result: { isError: false, preview: 'ok' } } })} selected={false} episodePhase={null} hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
    expect(screen.getByText('cargo test')).toBeInTheDocument();
  });

  it('shows an error badge when the tool result errored', () => {
    render(<StreamCard card={card({ kind: 'tool', tool: { toolName: 'Edit', toolUseId: 't', inputSummary: 'a.ts', result: { isError: true, preview: 'boom' } } })} selected={false} episodePhase={null} hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText(/error/i)).toBeInTheDocument();
  });

  it('shows a finding marker when hasFinding is true', () => {
    render(<StreamCard card={card({})} selected={false} episodePhase={null} hasFinding onSelect={() => {}} />);
    expect(screen.getByLabelText(/finding/i)).toBeInTheDocument();
  });

  it('shows the episode phase chip when provided', () => {
    render(<StreamCard card={card({})} selected={false} episodePhase="repair" hasFinding={false} onSelect={() => {}} />);
    expect(screen.getByText('repair')).toBeInTheDocument();
  });

  it('marks the selected card and fires onSelect with the event id on click', () => {
    const onSelect = vi.fn();
    render(<StreamCard card={card({ eventId: 'evt-1' })} selected onSelect={onSelect} episodePhase={null} hasFinding={false} />);
    const el = screen.getByTestId('stream-card');
    expect(el.getAttribute('data-selected')).toBe('true');
    fireEvent.click(el);
    expect(onSelect).toHaveBeenCalledWith('evt-1');
  });
});
