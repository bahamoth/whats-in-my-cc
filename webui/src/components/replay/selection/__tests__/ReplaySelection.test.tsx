/**
 * PR-6 RED — ReplaySelection is the single source of truth for who is
 * "selected" in the replay view. It backs Waterfall, Graph (PR-7),
 * WhyPanel, and BottomDrawer with one state, and mirrors the state into
 * URL search params so deep-links round-trip.
 *
 * Keys we lock in (plan §10.1 PR-6):
 *   ?selected=<nodeId>  ?finding=<findingId>  ?why=open|closed
 *   ?raw=open|closed
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
      wrapper: wrap('/sessions/s1?selected=node-a&why=open'),
    });
    expect(result.current.selectedNodeId).toBe('node-a');
    expect(result.current.whyPanelOpen).toBe(true);
  });

  it('defaults to no selection and closed panels when params absent', () => {
    const { result } = renderHook(() => useReplaySelection(), {
      wrapper: wrap('/sessions/s1'),
    });
    expect(result.current.selectedNodeId).toBeNull();
    expect(result.current.selectedFindingId).toBeNull();
    expect(result.current.whyPanelOpen).toBe(false);
    expect(result.current.rawDrawerOpen).toBe(false);
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

  it('openWhyPanel and closeWhyPanel toggle the URL param', () => {
    function Probe() {
      const sel = useReplaySelection();
      const loc = useLocation();
      return (
        <div>
          <button onClick={sel.openWhyPanel}>open</button>
          <button onClick={sel.closeWhyPanel}>close</button>
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
    act(() => screen.getByText('open').click());
    expect(screen.getByTestId('search').textContent).toContain('why=open');
    act(() => screen.getByText('close').click());
    expect(screen.getByTestId('search').textContent).not.toContain('why=open');
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
