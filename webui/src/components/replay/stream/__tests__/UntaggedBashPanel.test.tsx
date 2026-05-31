import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { UntaggedBashPanel } from '../UntaggedBashPanel';

const bash = (command: string, id: string) => ({ event_id: id, kind: 'tool_call', tool_name: 'Bash', observed_at: '2026-05-31T00:00:00Z', payload: { input: { command } } });

describe('UntaggedBashPanel', () => {
  it('is hidden by default and toggles open to show unmatched tokens with count + hint', () => {
    const events = [bash('gh pr view', '1'), bash('gh pr list', '2'), bash('grep x', '3')] as any;
    render(<UntaggedBashPanel events={events} />);
    expect(screen.queryByTestId('untagged-list')).toBeNull(); // hidden
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    // token, sample, and hint all contain 'gh' → assert presence via getAllByText
    expect(screen.getAllByText(/gh/).length).toBeGreaterThan(0);
    expect(screen.getByText(/×2/)).toBeInTheDocument();
    expect(screen.getByText(/BASH_FIRST_TOKEN_TAGS/)).toBeInTheDocument();
    expect(screen.queryByText(/grep/)).toBeNull(); // matched → excluded
  });
  it('shows nothing-to-do when all matched', () => {
    render(<UntaggedBashPanel events={[bash('grep x', '1')] as any} />);
    fireEvent.click(screen.getByTestId('untagged-toggle'));
    expect(screen.getByText(/all Bash patterns tagged/i)).toBeInTheDocument();
  });
});
