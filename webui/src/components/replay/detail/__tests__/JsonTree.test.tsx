// webui/src/components/replay/detail/__tests__/JsonTree.test.tsx
/**
 * R3 RED — JsonTree is a controlled collapsible JSON renderer. Expansion is
 * owned by the parent (Set<string> of open paths) so it survives re-render.
 * Plan R3 Task 1 / spec §4 (#2 persistence).
 */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { JsonTree } from '../JsonTree';

describe('JsonTree', () => {
  it('renders primitive leaves with their key and value', () => {
    render(<JsonTree data={{ a: 1, b: 'x' }} expanded={new Set(['$'])} onToggle={() => {}} />);
    expect(screen.getByText('a')).toBeInTheDocument();
    expect(screen.getByText('1')).toBeInTheDocument();
    expect(screen.getByText('b')).toBeInTheDocument();
    expect(screen.getByText('"x"')).toBeInTheDocument();
  });

  it('hides children of a collapsed object', () => {
    // root '$' open, but nested '$.obj' closed
    render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$'])} onToggle={() => {}} />);
    expect(screen.getByText('obj')).toBeInTheDocument();
    expect(screen.queryByText('deep')).toBeNull();
  });

  it('shows children when the path is in the expanded set', () => {
    render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$', '$.obj'])} onToggle={() => {}} />);
    expect(screen.getByText('deep')).toBeInTheDocument();
  });

  it('fires onToggle with the node path when a collapsible key is clicked', () => {
    const onToggle = vi.fn();
    render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$'])} onToggle={onToggle} />);
    fireEvent.click(screen.getByText('obj'));
    expect(onToggle).toHaveBeenCalledWith('$.obj');
  });

  it('preserves displayed expansion across a re-render with a new data reference (same shape)', () => {
    const { rerender } = render(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$', '$.obj'])} onToggle={() => {}} />);
    expect(screen.getByText('deep')).toBeInTheDocument();
    // simulate a refetch handing a brand-new object with identical shape
    rerender(<JsonTree data={{ obj: { deep: 1 } }} expanded={new Set(['$', '$.obj'])} onToggle={() => {}} />);
    expect(screen.getByText('deep')).toBeInTheDocument();
  });
});
