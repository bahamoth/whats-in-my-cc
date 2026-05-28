/**
 * PR-6 — single source of truth for replay-view selection state.
 *
 * URL-backed (deep-linkable): selected, finding, why, raw.
 * In-memory only: hoveredNodeId (would flood URL history with hover noise).
 */
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import { useSearchParams } from 'react-router-dom';

interface ReplaySelectionState {
  selectedNodeId: string | null;
  selectedFindingId: string | null;
  hoveredNodeId: string | null;
  whyPanelOpen: boolean;
  rawDrawerOpen: boolean;
  setSelectedNodeId: (id: string | null) => void;
  setSelectedFindingId: (id: string | null) => void;
  setHoveredNodeId: (id: string | null) => void;
  openWhyPanel: () => void;
  closeWhyPanel: () => void;
  openRawDrawer: () => void;
  closeRawDrawer: () => void;
}

const Ctx = createContext<ReplaySelectionState | null>(null);

export function ReplaySelectionProvider({ children }: { children: ReactNode }) {
  const [params, setParams] = useSearchParams();
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);

  const update = useCallback(
    (mutate: (p: URLSearchParams) => void) => {
      const next = new URLSearchParams(params);
      mutate(next);
      setParams(next, { replace: true });
    },
    [params, setParams],
  );

  const value = useMemo<ReplaySelectionState>(() => {
    const selectedNodeId = params.get('selected');
    const selectedFindingId = params.get('finding');
    const whyPanelOpen = params.get('why') === 'open';
    const rawDrawerOpen = params.get('raw') === 'open';

    return {
      selectedNodeId,
      selectedFindingId,
      hoveredNodeId,
      whyPanelOpen,
      rawDrawerOpen,
      setSelectedNodeId: (id) =>
        update((p) => (id ? p.set('selected', id) : p.delete('selected'))),
      setSelectedFindingId: (id) =>
        update((p) => (id ? p.set('finding', id) : p.delete('finding'))),
      setHoveredNodeId,
      openWhyPanel: () => update((p) => p.set('why', 'open')),
      closeWhyPanel: () => update((p) => p.delete('why')),
      openRawDrawer: () => update((p) => p.set('raw', 'open')),
      closeRawDrawer: () => update((p) => p.delete('raw')),
    };
  }, [params, hoveredNodeId, update]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useReplaySelection(): ReplaySelectionState {
  const v = useContext(Ctx);
  if (!v) throw new Error('useReplaySelection must be used inside ReplaySelectionProvider');
  return v;
}
