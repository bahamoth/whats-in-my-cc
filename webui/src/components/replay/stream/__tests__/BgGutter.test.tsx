import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { BgGutter } from '../BgGutter';

describe('BgGutter', () => {
  it('renders one rail per cell with a start glyph on the start marker', () => {
    render(
      <BgGutter
        row={{
          cells: [
            { lane: 0, agentId: 'a', color: '#7da7ff', marker: 'start' },
            { lane: 1, agentId: 'b', color: '#41c285', marker: 'mid' },
          ],
          dense: 0,
        }}
      />,
    );
    expect(screen.getAllByTestId('gutter-rail')).toHaveLength(2);
    expect(screen.getByTestId('gutter-start')).toBeInTheDocument();
    expect(screen.queryByTestId('gutter-end')).toBeNull();
  });

  it('renders an end glyph on the end marker', () => {
    render(
      <BgGutter row={{ cells: [{ lane: 0, agentId: 'a', color: '#7da7ff', marker: 'end' }], dense: 0 }} />,
    );
    expect(screen.getByTestId('gutter-end')).toBeInTheDocument();
  });

  it('dense → single neutral spine, no per-agent rails', () => {
    render(<BgGutter row={{ cells: [], dense: 5 }} />);
    expect(screen.queryAllByTestId('gutter-rail')).toHaveLength(0);
    expect(screen.getByTestId('gutter-dense')).toBeInTheDocument();
  });

  it('no row → empty gutter element (keeps the column width)', () => {
    render(<BgGutter row={undefined} />);
    expect(screen.getByTestId('gutter')).toBeInTheDocument();
    expect(screen.queryAllByTestId('gutter-rail')).toHaveLength(0);
  });

  // A+B unified timeline (2026-06-14): the gutter cell IS the time-spine. Every
  // row draws a continuous main spine line (so rows read as one axis) and a
  // node colored by the row's primary kind. Background-subagent lanes branch off
  // the same spine (the gutter is absorbed into the spine).
  it('always renders the continuous main time-spine line', () => {
    render(<BgGutter row={undefined} />);
    expect(screen.getByTestId('spine-line')).toBeInTheDocument();
  });

  it('renders a kind-colored node on the spine for the row', () => {
    render(<BgGutter row={undefined} kind="user" />);
    const node = screen.getByTestId('spine-node');
    expect(node).toBeInTheDocument();
    expect(node).toHaveAttribute('data-kind', 'user');
  });

  it('omits the spine node when the row has no primary kind', () => {
    render(<BgGutter row={undefined} kind={null} />);
    expect(screen.queryByTestId('spine-node')).toBeNull();
  });

  it('renders the row clock time (HH:MM) on the spine when given timeMs', () => {
    render(<BgGutter row={undefined} kind="user" timeMs={new Date('2026-06-14T09:06:09Z').getTime()} />);
    const t = screen.getByTestId('spine-time');
    // local timezone-independent: just assert it's a HH:MM label
    expect(t.textContent ?? '').toMatch(/^\d{1,2}:\d{2}$/);
  });

  it('omits the clock time when timeMs is missing', () => {
    render(<BgGutter row={undefined} kind="user" />);
    expect(screen.queryByTestId('spine-time')).toBeNull();
  });
});
