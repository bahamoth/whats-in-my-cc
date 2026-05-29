// webui/src/components/replay/detail/__tests__/RawTab.test.tsx
/**
 * R3 RED — RawTab persists JsonTree expansion per node across re-renders /
 * data refreshes (the #2 regression lock). Plan R3 Task 2 / spec §4.
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RawTab } from '../RawTab';

describe('RawTab', () => {
  it('renders the raw record as a tree (root open by default)', () => {
    render(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    expect(screen.getByText('outer')).toBeInTheDocument();
  });

  it('keeps a node expanded after a re-render with a new record reference', () => {
    const { rerender } = render(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    fireEvent.click(screen.getByText('outer')); // expand $.outer
    expect(screen.getByText('inner')).toBeInTheDocument();
    // refetch hands a fresh object of the same shape
    rerender(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    expect(screen.getByText('inner')).toBeInTheDocument();
  });

  it('shows an empty hint when there is no record', () => {
    render(<RawTab nodeId={null} record={null} />);
    expect(screen.getByText(/no raw record|select/i)).toBeInTheDocument();
  });
});
