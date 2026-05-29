/**
 * R5 — Focused neighborhood subgraph. Shows the 1–2 hop causal neighborhood
 * of the selected node using dagre LR layout + @xyflow/react. Deterministic
 * from neighborhood() so data-* counts are always exact.
 */
import { useState, useMemo, type CSSProperties } from 'react';
import { ReactFlow, ReactFlowProvider, type Node, type Edge } from '@xyflow/react';
import dagre from 'dagre';
import '@xyflow/react/dist/style.css';
import type { GraphNodeDto, GraphEdgeDto } from '../../../api/types';
import { neighborhood } from './neighborhood';
import { causalEdgeStyle } from '../causalEdgeStyle';
import { nodeLabel } from '../stream/nodeLabel';
import styles from './FocusedInsightGraph.module.css';

const NODE_WIDTH = 150;
const NODE_HEIGHT = 40;

export interface FocusedInsightGraphProps {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
  selectedNodeId: string | null;
  onSelectNode: (id: string) => void;
}

function buildLayout(
  subNodes: GraphNodeDto[],
  subEdges: GraphEdgeDto[],
  selectedNodeId: string | null,
): { rfNodes: Node[]; rfEdges: Edge[] } {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'LR', nodesep: 28, ranksep: 56 });
  g.setDefaultEdgeLabel(() => ({}));

  for (const node of subNodes) {
    g.setNode(node.node_id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }
  for (const edge of subEdges) {
    if (g.hasNode(edge.from_node_id) && g.hasNode(edge.to_node_id)) {
      g.setEdge(edge.from_node_id, edge.to_node_id);
    }
  }
  dagre.layout(g);

  const rfNodes: Node[] = subNodes.map((node) => {
    const pos = g.node(node.node_id);
    const isCenter = node.node_id === selectedNodeId;
    return {
      id: node.node_id,
      position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 },
      data: {
        label: (() => {
          const lbl = nodeLabel({ node_kind: node.node_kind, payload: node.payload });
          return (
            <div
              className={`${styles.nodeInner} ${isCenter ? styles.center : ''}`}
              data-node-kind={node.node_kind}
              data-center={isCenter ? 'true' : 'false'}
            >
              <span className={styles.primary}>{lbl.primary}</span>
              {lbl.secondary && (
                <span className={styles.secondary}>{lbl.secondary}</span>
              )}
              <span className={styles.nodeId}>{node.node_kind}</span>
            </div>
          );
        })(),
      },
      style: {
        width: NODE_WIDTH,
        height: NODE_HEIGHT,
        padding: 0,
        border: isCenter
          ? '2px solid var(--witmcc-accent, #4f8cff)'
          : '1px solid var(--witmcc-border-strong, #2a3040)',
        background: isCenter
          ? 'var(--witmcc-accent-soft, #1f3a78)'
          : 'var(--witmcc-surface-2, #161a23)',
        borderRadius: 4,
        cursor: 'pointer',
      },
    } satisfies Node;
  });

  const rfEdges: Edge[] = subEdges.map((edge) => {
    const style = causalEdgeStyle({ origin: edge.origin, confidence: edge.confidence });
    const css: CSSProperties = {
      strokeWidth: style.strokeWidth,
      opacity: style.opacity,
    };
    if (style.strokeDasharray) css.strokeDasharray = style.strokeDasharray;
    return {
      id: edge.edge_id,
      source: edge.from_node_id,
      target: edge.to_node_id,
      label: edge.inference_rule_id ?? undefined,
      style: css,
      animated: false,
    } satisfies Edge;
  });

  return { rfNodes, rfEdges };
}

function FocusedInsightGraphInner({
  nodes,
  edges,
  selectedNodeId,
  onSelectNode,
}: FocusedInsightGraphProps) {
  const [hops, setHops] = useState(1);

  const sub = useMemo(
    () => neighborhood(nodes, edges, selectedNodeId, hops),
    [nodes, edges, selectedNodeId, hops],
  );

  const { rfNodes, rfEdges } = useMemo(
    () => buildLayout(sub.nodes, sub.edges, selectedNodeId),
    [sub, selectedNodeId],
  );

  if (!selectedNodeId || sub.nodes.length === 0) {
    return (
      <div className={styles.empty} role="status">
        Select a node to view its causal neighborhood.
      </div>
    );
  }

  return (
    <div
      className={styles.wrap}
      data-testid="focused-graph"
      data-node-count={sub.nodes.length}
      data-edge-count={sub.edges.length}
      data-hops={hops}
    >
      <div className={styles.toolbar}>
        <button
          data-testid="hop-toggle"
          className={styles.hopToggle}
          onClick={() => setHops((h) => (h === 1 ? 2 : 1))}
          aria-label={hops === 1 ? 'Expand to 2 hops' : 'Collapse to 1 hop'}
        >
          {hops === 1 ? '2 hops' : '1 hop'}
        </button>
      </div>
      <ReactFlow
        nodes={rfNodes}
        edges={rfEdges}
        fitView
        proOptions={{ hideAttribution: true }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        onNodeClick={(_evt, node) => onSelectNode(node.id)}
      />
    </div>
  );
}

export function FocusedInsightGraph(props: FocusedInsightGraphProps) {
  return (
    <ReactFlowProvider>
      <FocusedInsightGraphInner {...props} />
    </ReactFlowProvider>
  );
}
