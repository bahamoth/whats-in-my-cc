import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { UntaggedBashPanel } from '../UntaggedBashPanel';

const bash = (command: string, id: string) => ({ event_id: id, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } });

describe('UntaggedBashPanel', () => {
  it('is hidden by default and toggles open to show unmatched tokens with count + hint', () => {
    // `frobnicate` is an unknown command → unmatched; `grep` is tagged read.file → excluded.
    const events = [bash('frobnicate view', '1'), bash('frobnicate list', '2'), bash('grep x', '3')] as any;
    render(<UntaggedBashPanel events={events} />);
    expect(screen.queryByTestId('untagged-list')).toBeNull(); // hidden
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    expect(screen.getAllByText(/frobnicate/).length).toBeGreaterThan(0);
    expect(screen.getByText(/×2/)).toBeInTheDocument();
    expect(screen.getByText(/BASH_FIRST_TOKEN_TAGS/)).toBeInTheDocument();
    expect(screen.queryByText(/grep/)).toBeNull(); // matched → excluded
  });
  it('shows nothing-to-do when all matched', () => {
    render(<UntaggedBashPanel events={[bash('grep x', '1')] as any} />);
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    expect(screen.getByText(/all Bash patterns tagged/i)).toBeInTheDocument();
  });

  it('jumps to the first-occurrence card and closes when a row link is clicked', () => {
    const onJump = vi.fn();
    const events = [bash('frobnicate view', 'ev-1'), bash('frobnicate list', 'ev-2')] as any;
    render(<UntaggedBashPanel events={events} onJump={onJump} />);
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    fireEvent.click(screen.getByTestId('untagged-jump-frobnicate'));
    expect(onJump).toHaveBeenCalledWith('ev-1'); // first 'frobnicate' occurrence
    // clicking jump closes the panel so the card is visible
    expect(screen.queryByTestId('untagged-list')).toBeNull();
  });
});
