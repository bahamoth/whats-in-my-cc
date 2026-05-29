/**
 * PR-6 RED — ReplaySelection is the single source of truth for who is
 * "selected" in the replay view. It backs Waterfall and Graph (PR-7) with one
 * state, and mirrors the state into URL search params so deep-links round-trip.
 *
 * R3 — the `why`/`raw` params were removed with the WhyPanel + raw drawer.
 *
 * Keys we lock in (plan §10.1 PR-6, trimmed by R3):
 *   ?selected=<nodeId>  ?finding=<findingId>
 */
import { describe, expect, it } from 'vitest';
import { act, render, renderHook, screen } from '@testing-library/react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';
import {
  ReplaySelectionProvider,
  useReplaySelection,
} from '../ReplaySelection';

function wrap(initialUrl: string) {
  return ({ children }: { children: React.ReactNode }) => (
    <MemoryRouter initialEntries={[initialUrl]}>
      <Routes>
        <Route
          path="/sessions/:id"
          element={<ReplaySelectionProvider>{children}</ReplaySelectionProvider>}
        />
      </Routes>
    </MemoryRouter>
  );
}

describe('ReplaySelectionProvider', () => {
  it('exposes selection state from the URL on initial render', () => {
    const { result } = renderHook(() => useReplaySelection(), {
      wrapper: wrap('/sessions/s1?selected=node-a&finding=f-1'),
    });
    expect(result.current.selectedNodeId).toBe('node-a');
    expect(result.current.selectedFindingId).toBe('f-1');
  });

  it('defaults to no selection when params absent', () => {
    const { result } = renderHook(() => useReplaySelection(), {
      wrapper: wrap('/sessions/s1'),
    });
    expect(result.current.selectedNodeId).toBeNull();
    expect(result.current.selectedFindingId).toBeNull();
    expect(result.current.hoveredNodeId).toBeNull();
  });

  it('setSelectedNodeId updates URL search params', () => {
    function Probe() {
      const sel = useReplaySelection();
      const loc = useLocation();
      return (
        <div>
          <button onClick={() => sel.setSelectedNodeId('node-b')}>set</button>
          <span data-testid="search">{loc.search}</span>
        </div>
      );
    }
    render(
      <MemoryRouter initialEntries={['/sessions/s1']}>
        <Routes>
          <Route
            path="/sessions/:id"
            element={
              <ReplaySelectionProvider>
                <Probe />
              </ReplaySelectionProvider>
            }
          />
        </Routes>
      </MemoryRouter>,
    );
    act(() => {
      screen.getByText('set').click();
    });
    expect(screen.getByTestId('search').textContent).toContain('selected=node-b');
  });

  it('clearing selection removes the URL param entirely', () => {
    function Probe() {
      const sel = useReplaySelection();
      const loc = useLocation();
      return (
        <div>
          <button onClick={() => sel.setSelectedNodeId(null)}>clear</button>
          <span data-testid="search">{loc.search}</span>
        </div>
      );
    }
    render(
      <MemoryRouter initialEntries={['/sessions/s1?selected=node-a']}>
        <Routes>
          <Route
            path="/sessions/:id"
            element={
              <ReplaySelectionProvider>
                <Probe />
              </ReplaySelectionProvider>
            }
          />
        </Routes>
      </MemoryRouter>,
    );
    act(() => screen.getByText('clear').click());
    expect(screen.getByTestId('search').textContent).not.toContain('selected=');
  });

  it('hoveredNodeId is in-memory only (not mirrored to URL)', () => {
    function Probe() {
      const sel = useReplaySelection();
      const loc = useLocation();
      return (
        <div>
          <button onClick={() => sel.setHoveredNodeId('hov-1')}>hover</button>
          <span data-testid="search">{loc.search}</span>
          <span data-testid="hovered">{sel.hoveredNodeId ?? ''}</span>
        </div>
      );
    }
    render(
      <MemoryRouter initialEntries={['/sessions/s1']}>
        <Routes>
          <Route
            path="/sessions/:id"
            element={
              <ReplaySelectionProvider>
                <Probe />
              </ReplaySelectionProvider>
            }
          />
        </Routes>
      </MemoryRouter>,
    );
    act(() => screen.getByText('hover').click());
    expect(screen.getByTestId('hovered').textContent).toBe('hov-1');
    expect(screen.getByTestId('search').textContent).not.toContain('hovered');
  });
});
