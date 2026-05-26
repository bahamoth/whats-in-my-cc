import { useMemo } from 'react';
import type { GraphPayload, GraphNodeDto } from '../api/types';
import { LANES, laneForNodeKind } from '../api/laneMapping';
import styles from './Timeline.module.css';

type Props = {
  graph: GraphPayload;
  selectedNodeId: string | null;
  onSelect: (nodeId: string) => void;
};

const ROW_HEIGHT = 56;
const HEADER_WIDTH = 96;
const NODE_RADIUS = 6;

const PLACEHOLDERS: Partial<Record<(typeof LANES)[number], string>> = {
  Files: 'no file edits in this session',
  Hook: 'no hook events observed in this session',
  OTel: 'no OTel observed in this session',
  Quality: 'no findings yet',
};

export function Timeline({ graph, selectedNodeId, onSelect }: Props) {
  const { layout, width, height } = useMemo(() => buildLayout(graph), [graph]);
  return (
    <div className={styles.wrap}>
      <svg width={width} height={height} role="img" aria-label="session timeline">
        {/* lane backgrounds + labels */}
        {LANES.map((lane, idx) => {
          const y = idx * ROW_HEIGHT;
          return (
            <g key={lane}>
              <rect x={0} y={y} width={width} height={ROW_HEIGHT}
                    className={idx % 2 ? styles.laneAlt : styles.lane} />
              <text x={8} y={y + ROW_HEIGHT / 2 + 4} className={styles.laneLabel}>
                {lane}
              </text>
              {PLACEHOLDERS[lane] && layout.byLane[lane].length === 0 && (
                <text x={HEADER_WIDTH + 16} y={y + ROW_HEIGHT / 2 + 4}
                      className={styles.placeholder}>
                  {PLACEHOLDERS[lane]}
                </text>
              )}
            </g>
          );
        })}
        {/* edges first so nodes sit on top */}
        {graph.edges.map((e) => {
          const a = layout.posByNodeId[e.from_node_id];
          const b = layout.posByNodeId[e.to_node_id];
          if (!a || !b) return null;
          const dashed = e.origin !== 'deterministic';
          const merged = (e.attributes as Record<string, unknown>)?.merged === true;
          let d: string;
          if (e.from_node_id === e.to_node_id) {
            // merged self-loop: small arc above the node
            d = `M ${a.x},${a.y} C ${a.x - 12},${a.y - 18} ${a.x + 12},${a.y - 18} ${a.x},${a.y}`;
          } else if (a.lane === b.lane) {
            d = `M ${a.x},${a.y} L ${b.x},${b.y}`;
          } else {
            const mx = (a.x + b.x) / 2;
            d = `M ${a.x},${a.y} L ${mx},${a.y} L ${mx},${b.y} L ${b.x},${b.y}`;
          }
          return (
            <path
              key={e.edge_id}
              d={d}
              data-testid="edge-path"
              className={merged ? styles.edgeMerged : dashed ? styles.edgeInferred : styles.edge}
              fill="none"
            />
          );
        })}
        {/* nodes */}
        {graph.nodes.map((n) => {
          const p = layout.posByNodeId[n.node_id];
          if (!p) return null;
          const selected = selectedNodeId === n.node_id;
          return (
            <circle
              key={n.node_id}
              cx={p.x}
              cy={p.y}
              r={selected ? NODE_RADIUS + 2 : NODE_RADIUS}
              data-testid="node-marker"
              data-node-id={n.node_id}
              className={`${styles.node} ${styles[`node_${n.node_kind}` as keyof typeof styles] ?? ''} ${selected ? styles.selected : ''}`}
              onClick={() => onSelect(n.node_id)}
            >
              <title>{`${n.node_kind} · ${n.started_at}`}</title>
            </circle>
          );
        })}
      </svg>
    </div>
  );
}

type Layout = {
  byLane: Record<(typeof LANES)[number], GraphNodeDto[]>;
  posByNodeId: Record<string, { x: number; y: number; lane: (typeof LANES)[number] }>;
};

function buildLayout(graph: GraphPayload): { layout: Layout; width: number; height: number } {
  const byLane = {
    Intent: [], Context: [], Action: [], State: [], Files: [], Hook: [], OTel: [], Quality: [],
  } as Layout['byLane'];
  for (const n of graph.nodes) {
    const lane = laneForNodeKind(n.node_kind);
    if (lane) byLane[lane].push(n);
  }
  const allTimes = graph.nodes.map((n) => Date.parse(n.started_at));
  const minT = allTimes.length ? Math.min(...allTimes) : 0;
  const maxT = allTimes.length ? Math.max(...allTimes) : minT + 1;
  const span = Math.max(maxT - minT, 1);
  const innerWidth = 720;
  const width = HEADER_WIDTH + innerWidth + 32;
  const height = LANES.length * ROW_HEIGHT;
  const posByNodeId: Layout['posByNodeId'] = {};
  for (const lane of LANES) {
    const idx = LANES.indexOf(lane);
    const y = idx * ROW_HEIGHT + ROW_HEIGHT / 2;
    for (const n of byLane[lane]) {
      const t = Date.parse(n.started_at);
      const x = HEADER_WIDTH + 16 + ((t - minT) / span) * (innerWidth - 32);
      posByNodeId[n.node_id] = { x, y, lane };
    }
  }
  return { layout: { byLane, posByNodeId }, width, height };
}
