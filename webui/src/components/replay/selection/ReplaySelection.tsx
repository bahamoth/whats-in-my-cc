/**
 * PR-6 — single source of truth for replay-view selection state.
 *
 * URL-backed (deep-linkable): selected, finding.
 * In-memory only: hoveredNodeId (would flood URL history with hover noise).
 *
 * R3 — the `why`/`raw` URL params and their open/close actions were dropped
 * with the WhyPanel + SourcePanel/raw-drawer; the DetailPanel owns its own tab
 * state instead.
 */
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import { useSearchParams } from 'react-router-dom';

interface ReplaySelectionState {
  selectedNodeId: string | null;
  selectedFindingId: string | null;
  hoveredNodeId: string | null;
  setSelectedNodeId: (id: string | null) => void;
  setSelectedFindingId: (id: string | null) => void;
  setHoveredNodeId: (id: string | null) => void;
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

    return {
      selectedNodeId,
      selectedFindingId,
      hoveredNodeId,
      setSelectedNodeId: (id) =>
        update((p) => (id ? p.set('selected', id) : p.delete('selected'))),
      setSelectedFindingId: (id) =>
        update((p) => (id ? p.set('finding', id) : p.delete('finding'))),
      setHoveredNodeId,
    };
  }, [params, hoveredNodeId, update]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useReplaySelection(): ReplaySelectionState {
  const v = useContext(Ctx);
  if (!v) throw new Error('useReplaySelection must be used inside ReplaySelectionProvider');
  return v;
}
