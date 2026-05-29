// webui/src/components/replay/timeline/Timeline.tsx
/**
 * R4 — Timeline SVG surface.
 * Plan: docs/superpowers/plans/2026-05-29-witmcc-redesign-v2-R4-timeline.md Task 4.
 * Spec: §5 (time-series UI), §7 (memory / density cap).
 */
import { useCallback, useMemo, useRef, useState } from 'react';
import type { GraphNodeDto, GraphEdgeDto, EpisodeDto } from '../../../api/types';
import { LANES } from '../../../api/laneMapping';
import { causalEdgeStyle } from '../causalEdgeStyle';
import { useMediaQuery } from '../../../hooks/useMediaQuery';
import { makeTimeScale } from './timeScale';
import { fit, zoomAt, pan, clamp, type Viewport } from './viewport';
import { nodesByLane } from './nodeLane';
import styles from './Timeline.module.css';

export interface TimelineProps {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  episodes: EpisodeDto[];
  selectedNodeId: string | null;
  onSelect: (nodeId: string | null) => void;
  width?: number;
  height?: number;
}

const MAX_NODES_PER_LANE = 200;

// Layout constants
const AXIS_HEIGHT = 24;      // px for the time axis row
const EPISODE_HEIGHT = 12;   // px for the episode band row
const LANE_LABEL_W = 52;     // px for lane label column
const CTRL_HEIGHT = 30;      // px for control bar (rendered as HTML, not SVG)
const NODE_RADIUS = 5;       // circle radius for instant nodes
const NODE_BAR_H = 8;        // bar height for span nodes

// Lane row height
function laneHeight(svgHeight: number): number {
  const availH = svgHeight - AXIS_HEIGHT - EPISODE_HEIGHT;
  return Math.max(20, Math.floor(availH / LANES.length));
}

// Lane colours from CSS tokens; fallbacks match the token values in tokens.css
const LANE_COLORS: Record<string, string> = {
  Intent:  'var(--witmcc-lane-intent,  #7da7ff)',
  Context: 'var(--witmcc-lane-context, #b07dff)',
  Action:  'var(--witmcc-lane-action,  #41c285)',
  State:   'var(--witmcc-lane-state,   #f0b429)',
  Files:   'var(--witmcc-lane-files,   #ef4747)',
  Hook:    'var(--witmcc-lane-hook,    #2bd0d0)',
  OTel:    'var(--witmcc-lane-otel,    #d97aff)',
  Quality: 'var(--witmcc-lane-quality, #ff8a4c)',
};

// Episode phase colours
const PHASE_COLORS: Record<string, string> = {
  intake:       'var(--witmcc-phase-intake,       #4f8cff)',
  exploration:  'var(--witmcc-phase-exploration,  #b07dff)',
  diagnosis:    'var(--witmcc-phase-diagnosis,    #f0b429)',
  action:       'var(--witmcc-phase-action,       #41c285)',
  verification: 'var(--witmcc-phase-verification, #2bd0d0)',
  repair:       'var(--witmcc-phase-repair,       #ff8a4c)',
  drift:        'var(--witmcc-phase-drift,        #ef4747)',
  stall:        'var(--witmcc-phase-stall,        #6a7180)',
};

interface TooltipState {
  visible: boolean;
  x: number;
  y: number;
  nodeId: string;
  nodeKind: string;
  startedAt: string;
}

export function Timeline({
  nodes,
  edges,
  episodes,
  selectedNodeId,
  onSelect,
  width = 800,
  height = 300,
}: TimelineProps) {
  const reducedMotion = useMediaQuery('(prefers-reduced-motion: reduce)');

  // Compute time extent from nodes
  const extent = useMemo<[number, number]>(() => {
    if (nodes.length === 0) {
      const now = Date.now();
      return [now - 60_000, now];
    }
    let min = Infinity, max = -Infinity;
    for (const n of nodes) {
      const t0 = new Date(n.started_at).getTime();
      const t1 = n.ended_at ? new Date(n.ended_at).getTime() : t0;
      if (t0 < min) min = t0;
      if (t1 > max) max = t1;
    }
    // Guard against min === max (all at same time)
    if (min === max) { min -= 1000; max += 1000; }
    return [min, max];
  }, [nodes]);

  const [viewport, setViewport] = useState<Viewport>(() => fit(extent));

  // Re-fit if extent changes significantly (e.g., new session loaded)
  const extentKey = `${extent[0]}-${extent[1]}`;
  const prevExtentKey = useRef(extentKey);
  if (prevExtentKey.current !== extentKey) {
    prevExtentKey.current = extentKey;
    setViewport(fit(extent));
  }

  const [tooltip, setTooltip] = useState<TooltipState>({ visible: false, x: 0, y: 0, nodeId: '', nodeKind: '', startedAt: '' });

  // SVG drawable area
  const svgW = width;
  const svgH = height - CTRL_HEIGHT;
  const drawW = svgW - LANE_LABEL_W;
  const laneH = laneHeight(svgH);
  const axisY = svgH - AXIS_HEIGHT;

  // Time scale over current viewport
  const scale = useMemo(
    () => makeTimeScale([viewport.t0, viewport.t1], [LANE_LABEL_W, svgW]),
    [viewport.t0, viewport.t1, svgW]
  );

  // Axis ticks
  const ticks = useMemo(() => {
    const count = Math.max(2, Math.floor(drawW / 90));
    const fmt = scale.tickFormat(count);
    return scale.ticks(count).map((d) => ({ t: d.getTime(), x: scale(d), label: fmt(d) }));
  }, [scale, drawW]);

  // Group nodes by lane
  const byLane = useMemo(() => nodesByLane(nodes), [nodes]);

  // Visible nodes per lane (within viewport window)
  const visibleByLane = useMemo(() => {
    const result = new Map<string, GraphNodeDto[]>();
    for (const lane of LANES) {
      const all = byLane.get(lane) ?? [];
      const vis = all.filter((n) => {
        const t0 = new Date(n.started_at).getTime();
        const t1 = n.ended_at ? new Date(n.ended_at).getTime() : t0;
        return t1 >= viewport.t0 && t0 <= viewport.t1;
      });
      result.set(lane, vis);
    }
    return result;
  }, [byLane, viewport.t0, viewport.t1]);

  // Drag pan
  const dragRef = useRef<{ dragging: boolean; lastX: number; lastT: number }>({
    dragging: false, lastX: 0, lastT: 0,
  });

  const onMouseDown = useCallback((e: React.MouseEvent<SVGSVGElement>) => {
    // Only start drag on background (the SVG itself or lane rows, not nodes)
    dragRef.current = { dragging: true, lastX: e.clientX, lastT: viewport.t0 };
  }, [viewport.t0]);

  const onMouseMove = useCallback((e: React.MouseEvent<SVGSVGElement>) => {
    if (!dragRef.current.dragging) return;
    const dx = e.clientX - dragRef.current.lastX;
    // px → time
    const pxPerMs = drawW / (viewport.t1 - viewport.t0);
    const deltaT = -dx / pxPerMs;
    dragRef.current.lastX = e.clientX;
    setViewport((v) => clamp(pan(v, deltaT), extent));
  }, [viewport.t0, viewport.t1, drawW, extent]);

  const onMouseUp = useCallback(() => {
    dragRef.current.dragging = false;
  }, []);

  // Wheel zoom
  const onWheel = useCallback((e: React.WheelEvent<SVGSVGElement>) => {
    e.preventDefault();
    // Map cursor x to time
    const rect = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const pxInDraw = Math.max(0, px - LANE_LABEL_W);
    const frac = drawW > 0 ? pxInDraw / drawW : 0.5;
    const focusT = viewport.t0 + frac * (viewport.t1 - viewport.t0);
    const factor = e.deltaY < 0 ? 0.7 : 1 / 0.7;
    setViewport((v) => clamp(zoomAt(v, factor, focusT), extent));
  }, [viewport.t0, viewport.t1, drawW, extent]);

  // SVG background click → deselect
  const onSvgClick = useCallback((e: React.MouseEvent<SVGSVGElement>) => {
    if ((e.target as Element) === e.currentTarget) {
      onSelect(null);
    }
  }, [onSelect]);

  // Node click
  const onNodeClick = useCallback((e: React.MouseEvent, nodeId: string) => {
    e.stopPropagation();
    onSelect(nodeId);
  }, [onSelect]);

  // Node hover
  const onNodeEnter = useCallback((e: React.MouseEvent, n: GraphNodeDto) => {
    const el = e.currentTarget as SVGElement;
    setTooltip({
      visible: true,
      x: parseFloat(el.getAttribute('cx') ?? el.getAttribute('x') ?? '0') + LANE_LABEL_W,
      y: parseFloat(el.getAttribute('cy') ?? el.getAttribute('y') ?? '0'),
      nodeId: n.node_id,
      nodeKind: n.node_kind,
      startedAt: n.started_at,
    });
  }, []);

  const onNodeLeave = useCallback(() => {
    setTooltip((t) => ({ ...t, visible: false }));
  }, []);

  // Compute lane Y offsets (lanes rendered below episode band + some padding)
  const laneStartY = EPISODE_HEIGHT + 4;

  return (
    <div className={styles.root} style={{ width, height }}>
      {/* Control bar */}
      <div className={styles.controls}>
        <button
          className={styles.controlBtn}
          data-testid="zoom-in"
          onClick={() => setViewport((v) => clamp(zoomAt(v, 0.5, (v.t0 + v.t1) / 2), extent))}
        >
          +
        </button>
        <button
          className={styles.controlBtn}
          data-testid="zoom-out"
          onClick={() => setViewport((v) => clamp(zoomAt(v, 2, (v.t0 + v.t1) / 2), extent))}
        >
          −
        </button>
        <button
          className={styles.controlBtn}
          data-testid="fit"
          onClick={() => setViewport(fit(extent))}
        >
          fit
        </button>
      </div>

      {/* SVG canvas */}
      <div className={styles.svgWrapper} style={{ width: svgW, height: svgH }}>
        <svg
          className={styles.svg}
          width={svgW}
          height={svgH}
          viewBox={`0 0 ${svgW} ${svgH}`}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={onMouseUp}
          onMouseLeave={onMouseUp}
          onWheel={onWheel}
          onClick={onSvgClick}
        >
          {/* ── Episode band ── */}
          <g data-testid="episode-band">
            {episodes.map((ep) => {
              const x0 = scale(new Date(ep.started_at));
              const x1 = scale(new Date(ep.ended_at));
              const w = Math.max(1, x1 - x0);
              const color = PHASE_COLORS[ep.phase] ?? 'var(--witmcc-fg-subtle, #6a7180)';
              return (
                <rect
                  key={ep.episode_id}
                  data-phase={ep.phase}
                  x={x0}
                  y={0}
                  width={w}
                  height={EPISODE_HEIGHT}
                  fill={color}
                  opacity={0.25}
                />
              );
            })}
          </g>

          {/* ── Lane rows ── */}
          {LANES.map((lane, laneIdx) => {
            const y = laneStartY + laneIdx * laneH;
            const laneNodes = visibleByLane.get(lane) ?? [];
            const aggregated = laneNodes.length > MAX_NODES_PER_LANE;
            const color = LANE_COLORS[lane] ?? '#888';

            return (
              <g key={lane} data-lane={lane}>
                {/* Lane background stripe */}
                <rect
                  x={0}
                  y={y}
                  width={svgW}
                  height={laneH}
                  fill={laneIdx % 2 === 0
                    ? 'var(--witmcc-surface-1, #11141b)'
                    : 'var(--witmcc-surface-2, #161a23)'}
                  opacity={0.5}
                />
                {/* Lane label */}
                <text
                  x={4}
                  y={y + laneH / 2 + 4}
                  fontSize={9}
                  fill="var(--witmcc-fg-subtle, #6a7180)"
                >
                  {lane}
                </text>
                {/* Divider */}
                <line
                  x1={0} y1={y + laneH}
                  x2={svgW} y2={y + laneH}
                  stroke="var(--witmcc-border, #1d212c)"
                  strokeWidth={0.5}
                />

                {aggregated ? (
                  /* Density cap: aggregated marker */
                  <g data-aggregated="true">
                    {laneNodes.slice(0, 60).map((n) => {
                      const tx = scale(new Date(n.started_at));
                      return (
                        <line
                          key={n.node_id}
                          x1={tx} y1={y + 2}
                          x2={tx} y2={y + laneH - 2}
                          stroke={color}
                          strokeWidth={1}
                          opacity={0.4}
                        />
                      );
                    })}
                    <text
                      x={LANE_LABEL_W + 4}
                      y={y + laneH / 2 + 4}
                      fontSize={9}
                      fill="var(--witmcc-fg-subtle, #6a7180)"
                    >
                      {laneNodes.length} nodes (aggregated)
                    </text>
                  </g>
                ) : (
                  /* Individual nodes */
                  laneNodes.map((n) => {
                    const isSelected = selectedNodeId === n.node_id;
                    const isInstant = !n.ended_at;
                    const tx = scale(new Date(n.started_at));
                    const cy = y + laneH / 2;

                    if (isInstant) {
                      return (
                        <circle
                          key={n.node_id}
                          data-node-id={n.node_id}
                          data-node-kind={n.node_kind}
                          data-selected={isSelected ? 'true' : undefined}
                          cx={tx}
                          cy={cy}
                          r={NODE_RADIUS}
                          fill={color}
                          opacity={isSelected ? 1 : 0.75}
                          stroke={isSelected ? 'var(--witmcc-fg, #e6e8ee)' : 'none'}
                          strokeWidth={isSelected ? 1.5 : 0}
                          style={{ cursor: 'pointer' }}
                          onClick={(e) => onNodeClick(e, n.node_id)}
                          onMouseEnter={(e) => onNodeEnter(e, n)}
                          onMouseLeave={onNodeLeave}
                        />
                      );
                    } else {
                      const tx1 = scale(new Date(n.ended_at!));
                      const barW = Math.max(2, tx1 - tx);
                      return (
                        <rect
                          key={n.node_id}
                          data-node-id={n.node_id}
                          data-node-kind={n.node_kind}
                          data-selected={isSelected ? 'true' : undefined}
                          x={tx}
                          y={cy - NODE_BAR_H / 2}
                          width={barW}
                          height={NODE_BAR_H}
                          rx={2}
                          fill={color}
                          opacity={isSelected ? 1 : 0.75}
                          stroke={isSelected ? 'var(--witmcc-fg, #e6e8ee)' : 'none'}
                          strokeWidth={isSelected ? 1.5 : 0}
                          style={{ cursor: 'pointer' }}
                          onClick={(e) => onNodeClick(e, n.node_id)}
                          onMouseEnter={(e) => onNodeEnter(e, n)}
                          onMouseLeave={onNodeLeave}
                        />
                      );
                    }
                  })
                )}
              </g>
            );
          })}

          {/* ── Edges ── */}
          <g>
            {edges.map((e) => {
              const fromNode = nodes.find((n) => n.node_id === e.from_node_id);
              const toNode = nodes.find((n) => n.node_id === e.to_node_id);
              if (!fromNode || !toNode) return null;

              const style = causalEdgeStyle({ origin: e.origin, confidence: e.confidence });
              const isInferred = e.origin === 'inferred';

              const isIncident = selectedNodeId
                ? e.from_node_id === selectedNodeId || e.to_node_id === selectedNodeId
                : null;
              const emphasized = selectedNodeId ? isIncident : undefined;
              const dimmed = selectedNodeId ? !isIncident : undefined;

              // Node positions
              const fromLaneIdx = LANES.indexOf(
                LANES.find((l) => {
                  const bucket = visibleByLane.get(l);
                  return bucket?.some((n) => n.node_id === e.from_node_id);
                }) ??
                (LANES.find((l) => (byLane.get(l) ?? []).some((n) => n.node_id === e.from_node_id)) ?? LANES[0])
              );
              const toLaneIdx = LANES.indexOf(
                LANES.find((l) => {
                  const bucket = visibleByLane.get(l);
                  return bucket?.some((n) => n.node_id === e.to_node_id);
                }) ??
                (LANES.find((l) => (byLane.get(l) ?? []).some((n) => n.node_id === e.to_node_id)) ?? LANES[0])
              );

              const x1 = scale(new Date(fromNode.started_at));
              const y1 = laneStartY + fromLaneIdx * laneH + laneH / 2;
              const x2 = scale(new Date(toNode.started_at));
              const y2 = laneStartY + toLaneIdx * laneH + laneH / 2;

              // Simple cubic bezier
              const d = `M ${x1} ${y1} C ${x1} ${(y1 + y2) / 2}, ${x2} ${(y1 + y2) / 2}, ${x2} ${y2}`;

              return (
                <path
                  key={e.edge_id}
                  data-edge-id={e.edge_id}
                  data-origin={e.origin}
                  data-rule-id={e.inference_rule_id ?? undefined}
                  data-emphasized={emphasized ? 'true' : undefined}
                  data-dimmed={dimmed ? 'true' : undefined}
                  d={d}
                  fill="none"
                  stroke="var(--witmcc-fg-subtle, #6a7180)"
                  strokeDasharray={style.strokeDasharray}
                  strokeWidth={style.strokeWidth}
                  opacity={style.opacity}
                  className={isInferred && !reducedMotion ? styles.inferredEdge : undefined}
                />
              );
            })}
          </g>

          {/* ── Time axis ── */}
          <g data-testid="time-axis" transform={`translate(0, ${axisY})`}>
            {/* Axis background */}
            <rect
              x={0}
              y={0}
              width={svgW}
              height={AXIS_HEIGHT}
              fill="var(--witmcc-surface-2, #161a23)"
            />
            {/* Gridlines + ticks */}
            {ticks.map((tick) => (
              <g key={tick.t} transform={`translate(${tick.x}, 0)`}>
                <line
                  y1={-svgH + AXIS_HEIGHT}
                  y2={0}
                  stroke="var(--witmcc-border, #1d212c)"
                  strokeWidth={0.5}
                />
                <line y1={0} y2={4} stroke="var(--witmcc-fg-subtle, #6a7180)" strokeWidth={1} />
                <text
                  y={14}
                  textAnchor="middle"
                  fontSize={9}
                  fill="var(--witmcc-fg-subtle, #6a7180)"
                >
                  {tick.label}
                </text>
              </g>
            ))}
          </g>
        </svg>

        {/* Tooltip (HTML overlay for easy styling) */}
        {tooltip.visible && (
          <div
            data-testid="node-tooltip"
            className={styles.tooltip}
            style={{
              left: Math.min(tooltip.x + 8, svgW - 200),
              top: Math.max(0, tooltip.y - 32),
            }}
          >
            <strong>{tooltip.nodeKind}</strong> — {tooltip.nodeId}
            <br />
            <span style={{ color: 'var(--witmcc-fg-muted, #aab0bd)', fontSize: 10 }}>
              {new Date(tooltip.startedAt).toISOString()}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
