/**
 * PR-7 RED — ViewToggle switches between Waterfall and Graph by writing
 * the `view` URL param. Both views consume the same ReplaySelectionContext
 * so the selected node id survives the toggle.
 */
import { describe, expect, it } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';
import { ViewToggle } from '../ViewToggle';
import { ReplaySelectionProvider } from '../selection/ReplaySelection';

function renderAt(url: string) {
  function Probe() {
    const loc = useLocation();
    return <span data-testid="search">{loc.search}</span>;
  }
  return render(
    <MemoryRouter initialEntries={[url]}>
      <Routes>
        <Route
          path="/sessions/:id"
          element={
            <ReplaySelectionProvider>
              <>
                <ViewToggle />
                <Probe />
              </>
            </ReplaySelectionProvider>
          }
        />
      </Routes>
    </MemoryRouter>,
  );
}

describe('ViewToggle', () => {
  it('exposes a waterfall button and a graph button', () => {
    renderAt('/sessions/s1');
    expect(screen.getByRole('button', { name: /waterfall/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /graph/i })).toBeInTheDocument();
  });

  it('defaults to waterfall when no ?view param is set', () => {
    renderAt('/sessions/s1');
    const waterfall = screen.getByRole('button', { name: /waterfall/i });
    const graph = screen.getByRole('button', { name: /graph/i });
    expect(waterfall.getAttribute('aria-pressed')).toBe('true');
    expect(graph.getAttribute('aria-pressed')).toBe('false');
  });

  it('?view=graph marks the graph button as pressed', () => {
    renderAt('/sessions/s1?view=graph');
    const graph = screen.getByRole('button', { name: /graph/i });
    expect(graph.getAttribute('aria-pressed')).toBe('true');
  });

  it('clicking graph writes ?view=graph into the URL', () => {
    renderAt('/sessions/s1');
    fireEvent.click(screen.getByRole('button', { name: /graph/i }));
    expect(screen.getByTestId('search').textContent).toContain('view=graph');
  });

  it('clicking waterfall removes the ?view param entirely (default)', () => {
    renderAt('/sessions/s1?view=graph');
    fireEvent.click(screen.getByRole('button', { name: /waterfall/i }));
    expect(screen.getByTestId('search').textContent).not.toContain('view=');
  });
});
