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

  it('keeps a DEEP nested path expanded across repeated refetches (SSE churn)', () => {
    // The #2 complaint was "collapses on every screen refresh". This drives a
    // two-level expansion ($.message → $.message.usage) and then simulates an
    // SSE tick storm: three successive rerenders, each handing a brand-new
    // object of the same shape. The deep path must stay open the whole time.
    const fresh = () => ({ message: { usage: { input_tokens: 7 } } });
    const { rerender } = render(<RawTab nodeId="n1" record={fresh()} />);
    fireEvent.click(screen.getByText('message')); // expand $.message
    fireEvent.click(screen.getByText('usage')); // expand $.message.usage
    expect(screen.getByText('input_tokens')).toBeInTheDocument();

    for (let tick = 0; tick < 3; tick++) {
      rerender(<RawTab nodeId="n1" record={fresh()} />);
      expect(screen.getByText('usage')).toBeInTheDocument();
      expect(screen.getByText('input_tokens')).toBeInTheDocument();
    }
  });

  it('shows an empty hint when there is no record', () => {
    render(<RawTab nodeId={null} record={null} />);
    expect(screen.getByText(/no raw record|select/i)).toBeInTheDocument();
  });

  it('isolates expansion per node id', () => {
    const { rerender } = render(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    fireEvent.click(screen.getByText('outer')); // expand $.outer for n1
    expect(screen.getByText('inner')).toBeInTheDocument();

    // switch to a different node: its set is independent, so the nested
    // child is collapsed.
    rerender(<RawTab nodeId="n2" record={{ outer: { inner: 1 } }} />);
    expect(screen.getByText('outer')).toBeInTheDocument();
    expect(screen.queryByText('inner')).toBeNull();

    // back to n1: its expansion was preserved.
    rerender(<RawTab nodeId="n1" record={{ outer: { inner: 1 } }} />);
    expect(screen.getByText('inner')).toBeInTheDocument();
  });
});

describe('RawTab source-split blocks (Task 8)', () => {
  it('renders source-split blocks for entity + facets', () => {
    render(<RawTab nodeId="call" record={null} blocks={[
      { source: 'transcript', label: 'tool_call', record: { tool_name: 'Bash' } },
      { source: 'log_record', label: 'tool_result', record: { event_name: 'tool_result' } },
    ]} />);
    expect(screen.getByText(/transcript/)).toBeInTheDocument();
    expect(screen.getByText(/log_record/)).toBeInTheDocument();
  });

  it('falls back to single record when no blocks (back-compat)', () => {
    render(<RawTab nodeId="x" record={{ a: 1 }} />);
    // JsonTree renders the object key "a" as a span with the key class
    expect(screen.getByText('a')).toBeInTheDocument();
  });

  it('shows empty hint when neither blocks nor record', () => {
    render(<RawTab nodeId={null} record={null} />);
    expect(screen.getByText(/select a node/i)).toBeInTheDocument();
  });

  it('isolates expansion between two blocks with the same source but different index', () => {
    // key scheme is `${nodeId}:${source}:${i}` — two same-source blocks must
    // get independent expansion sets, so expanding one leaves the other closed.
    render(<RawTab nodeId="call" record={null} blocks={[
      { source: 'log_record', label: 'first', record: { outer: { inner: 'A' } } },
      { source: 'log_record', label: 'second', record: { outer: { inner: 'B' } } },
    ]} />);

    // both blocks render their own JsonTree: two `outer` toggles exist.
    const outers = screen.getAllByText('outer');
    expect(outers).toHaveLength(2);
    // neither nested child is visible yet (each block's root '$' open, $.outer closed)
    expect(screen.queryByText('"A"')).toBeNull();
    expect(screen.queryByText('"B"')).toBeNull();

    // expand $.outer in the FIRST block only
    fireEvent.click(outers[0]);
    expect(screen.getByText('"A"')).toBeInTheDocument();
    // the second block stays collapsed — its expansion set is independent
    expect(screen.queryByText('"B"')).toBeNull();
  });
});
