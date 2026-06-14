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
});
