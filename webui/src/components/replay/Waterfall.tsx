/**
 * PR-4 — Replay canvas. d3-scale based waterfall with one lane per
 * `LANES` entry and node rectangles whose width encodes duration.
 *
 * Drop-in for the legacy `Timeline`: same `graph / selectedNodeId / onSelect`
 * interface so `SessionDetailPage` can swap them without churn. Brush-zoom
 * and node virtualization are deferred to a follow-up PR — the current
 * implementation is O(N) over `graph.nodes` and is comfortable to ~5k
 * nodes (above that the SVG cost becomes visible, which is the trigger
 * for the next iteration).
 */
import { useMemo, type CSSProperties } from 'react';
import { scaleTime } from 'd3-scale';
import type { GraphPayload } from '../../api/types';
import { LANES, laneForNodeKind } from '../../api/laneMapping';
import styles from './Waterfall.module.css';

const LANE_HEIGHT = 56;
const LANE_LABEL_WIDTH = 96;
const NODE_HEIGHT = 14;
const POINT_RADIUS = 5;
const MIN_BAR_WIDTH = 4;
const INNER_PADDING = 16;

export interface WaterfallProps {
  graph: GraphPayload;
  selectedNodeId: string | null;
  onSelect: (nodeId: string) => void;
  /** Optional width override; defaults to 720 inner units. PR-8 will set
   *  this from a container ResizeObserver. */
  width?: number;
}

const PLACEHOLDERS: Partial<Record<(typeof LANES)[number], string>> = {
  Files: 'no file edits in this session',
  Hook: 'no hook events observed in this session',
  OTel: 'no OTel observed in this session',
  Quality: 'no findings yet',
};

export function Waterfall({ graph, selectedNodeId, onSelect, width = 720 }: WaterfallProps) {
  const { svgWidth, height, scale, perLaneCount } = useMemo(() => {
    const innerWidth = Math.max(240, width);
    const allTimes = graph.nodes.flatMap((n) => {
      const s = Date.parse(n.started_at);
      const e = n.ended_at ? Date.parse(n.ended_at) : s;
      return [s, e];
    });
    const minT = allTimes.length ? Math.min(...allTimes) : 0;
    const rawMax = allTimes.length ? Math.max(...allTimes) : minT + 1;
    const maxT = rawMax === minT ? minT + 1 : rawMax;
    const sc = scaleTime()
      .domain([new Date(minT), new Date(maxT)])
      .range([0, innerWidth - 2 * INNER_PADDING]);
    const svgWidth = LANE_LABEL_WIDTH + innerWidth;
    const height = LANES.length * LANE_HEIGHT;
    const perLaneCount: Record<string, number> = {};
    for (const n of graph.nodes) {
      const lane = laneForNodeKind(n.node_kind);
      if (lane) perLaneCount[lane] = (perLaneCount[lane] ?? 0) + 1;
    }
    return { svgWidth, height, scale: sc, perLaneCount };
  }, [graph, width]);

  return (
    <div className={styles.wrap}>
      <svg
        width={svgWidth}
        height={height}
        role="img"
        aria-label="session timeline"
        className={styles.svg}
      >
        {LANES.map((lane, idx) => {
          const y = idx * LANE_HEIGHT;
          const placeholder =
            (perLaneCount[lane] ?? 0) === 0 ? PLACEHOLDERS[lane] : undefined;
          return (
            <g key={lane}>
              <rect
                x={0}
                y={y}
                width={svgWidth}
                height={LANE_HEIGHT}
                className={idx % 2 ? styles.laneAlt : styles.lane}
              />
              <text x={8} y={y + LANE_HEIGHT / 2 + 4} className={styles.laneLabel}>
                {lane}
              </text>
              {placeholder && (
                <text
                  x={LANE_LABEL_WIDTH + INNER_PADDING}
                  y={y + LANE_HEIGHT / 2 + 4}
                  className={styles.placeholder}
                >
                  {placeholder}
                </text>
              )}
            </g>
          );
        })}

        {graph.nodes.map((n) => {
          const lane = laneForNodeKind(n.node_kind);
          if (!lane) return null;
          const laneIdx = LANES.indexOf(lane);
          const start = Date.parse(n.started_at);
          const end = n.ended_at ? Date.parse(n.ended_at) : start;
          const xStart = LANE_LABEL_WIDTH + INNER_PADDING + scale(new Date(start));
          const yCenter = laneIdx * LANE_HEIGHT + LANE_HEIGHT / 2;
          const selected = selectedNodeId === n.node_id;
          const hasDuration = end > start;
          const style: CSSProperties = { cursor: 'pointer' };

          if (hasDuration) {
            const w = Math.max(MIN_BAR_WIDTH, scale(new Date(end)) - scale(new Date(start)));
            return (
              <rect
                key={n.node_id}
                data-node-id={n.node_id}
                data-node-kind={n.node_kind}
                data-shape="bar"
                data-selected={selected ? 'true' : 'false'}
                x={xStart}
                y={yCenter - NODE_HEIGHT / 2}
                width={w}
                height={NODE_HEIGHT}
                rx={3}
                style={style}
                className={`${styles.bar} ${selected ? styles.selected : ''}`}
                onClick={() => onSelect(n.node_id)}
              >
                <title>{`${n.node_kind} · ${n.started_at}`}</title>
              </rect>
            );
          }
          return (
            <circle
              key={n.node_id}
              data-node-id={n.node_id}
              data-node-kind={n.node_kind}
              data-shape="point"
              data-selected={selected ? 'true' : 'false'}
              cx={xStart}
              cy={yCenter}
              r={POINT_RADIUS}
              style={style}
              className={`${styles.point} ${selected ? styles.selected : ''}`}
              onClick={() => onSelect(n.node_id)}
            >
              <title>{`${n.node_kind} · ${n.started_at}`}</title>
            </circle>
          );
        })}

        {/* edges: drawn after nodes so selection highlight still wins via CSS */}
        {graph.edges.map((e) => {
          // Only renderable when both endpoints have a position. We rely on
          // the natural DOM order: nodes were emitted above, so the edge is
          // visually above-the-lane-bg but below the node hit-area only
          // because <line> pointer-events are 'none' (set in module.css).
          const fromN = graph.nodes.find((n) => n.node_id === e.from_node_id);
          const toN = graph.nodes.find((n) => n.node_id === e.to_node_id);
          if (!fromN || !toN) return null;
          const fromLane = laneForNodeKind(fromN.node_kind);
          const toLane = laneForNodeKind(toN.node_kind);
          if (!fromLane || !toLane) return null;
          const x1 = LANE_LABEL_WIDTH + INNER_PADDING + scale(new Date(Date.parse(fromN.started_at)));
          const x2 = LANE_LABEL_WIDTH + INNER_PADDING + scale(new Date(Date.parse(toN.started_at)));
          const y1 = LANES.indexOf(fromLane) * LANE_HEIGHT + LANE_HEIGHT / 2;
          const y2 = LANES.indexOf(toLane) * LANE_HEIGHT + LANE_HEIGHT / 2;
          const isInferred = e.origin === 'inferred';
          const confidence = typeof e.confidence === 'number' ? e.confidence : 1;
          const strokeWidth = isInferred ? 1 + 2 * confidence : 1.5;
          const opacity = isInferred ? 0.35 + 0.5 * confidence : 0.7;
          return (
            <line
              key={e.edge_id}
              data-edge-id={e.edge_id}
              data-edge-origin={e.origin}
              x1={x1}
              y1={y1}
              x2={x2}
              y2={y2}
              strokeDasharray={isInferred ? '4 3' : undefined}
              strokeWidth={strokeWidth}
              opacity={opacity}
              className={styles.edge}
            />
          );
        })}
      </svg>
    </div>
  );
}
