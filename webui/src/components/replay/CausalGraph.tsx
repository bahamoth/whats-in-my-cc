/**
 * PR-7 — Causal graph view powered by React Flow with a dagre LR auto-
 * layout. Node clicks route through the same `onSelect(nodeId)` contract
 * as Waterfall so SessionDetailPage's selection state is shared without
 * extra wiring.
 *
 * Edge style is sourced from the shared `causalEdgeStyle` helper, so
 * deterministic ↔ inferred (and inferred-confidence scaling) cannot
 * drift between this view and Waterfall.
 */
import { useCallback, useMemo, type CSSProperties } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  type Node,
  type Edge,
  type NodeProps,
  ReactFlowProvider,
} from '@xyflow/react';
import dagre from 'dagre';
import '@xyflow/react/dist/style.css';
import type { GraphPayload, GraphNodeDto } from '../../api/types';
import { causalEdgeStyle } from './causalEdgeStyle';
import styles from './CausalGraph.module.css';

const NODE_WIDTH = 160;
const NODE_HEIGHT = 44;

interface CausalGraphProps {
  graph: GraphPayload;
  selectedNodeId: string | null;
  onSelect: (nodeId: string) => void;
}

interface NodeData {
  node: GraphNodeDto;
  selected: boolean;
  onSelect: (id: string) => void;
  [key: string]: unknown;
}

function CausalNode({ data }: NodeProps) {
  const d = data as unknown as NodeData;
  const sel = d.selected;
  return (
    <div
      data-node-id={d.node.node_id}
      data-node-kind={d.node.node_kind}
      data-selected={sel ? 'true' : 'false'}
      className={`${styles.node} ${sel ? styles.selected : ''}`}
      onClick={() => d.onSelect(d.node.node_id)}
    >
      <span className={styles.kind}>{d.node.node_kind}</span>
      <span className={styles.timestamp}>
        {new Date(d.node.started_at).toISOString().slice(11, 19)}
      </span>
    </div>
  );
}

const nodeTypes = { causal: CausalNode };

function layout(payload: GraphPayload): { nodes: Node[]; edges: Edge[] } {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'LR', nodesep: 32, ranksep: 64 });
  g.setDefaultEdgeLabel(() => ({}));

  for (const n of payload.nodes) {
    g.setNode(n.node_id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }
  for (const e of payload.edges) {
    if (g.hasNode(e.from_node_id) && g.hasNode(e.to_node_id)) {
      g.setEdge(e.from_node_id, e.to_node_id);
    }
  }
  dagre.layout(g);

  const nodes: Node[] = payload.nodes.map((n) => {
    const pos = g.node(n.node_id);
    return {
      id: n.node_id,
      type: 'causal',
      data: { node: n, selected: false, onSelect: () => {} } as unknown as Record<string, unknown>,
      position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 },
    } satisfies Node;
  });

  const edges: Edge[] = payload.edges.map((e) => {
    const style = causalEdgeStyle({ origin: e.origin, confidence: e.confidence });
    const css: CSSProperties = {
      strokeWidth: style.strokeWidth,
      opacity: style.opacity,
    };
    if (style.strokeDasharray) css.strokeDasharray = style.strokeDasharray;
    return {
      id: e.edge_id,
      source: e.from_node_id,
      target: e.to_node_id,
      label: e.inference_rule_id ?? undefined,
      style: css,
      animated: false,
    } satisfies Edge;
  });

  return { nodes, edges };
}

function CausalGraphInner({ graph, selectedNodeId, onSelect }: CausalGraphProps) {
  const handleSelect = useCallback((id: string) => onSelect(id), [onSelect]);

  const { nodes, edges } = useMemo(() => {
    const laid = layout(graph);
    const enriched: Node[] = laid.nodes.map((n) => ({
      ...n,
      data: {
        ...(n.data as Record<string, unknown>),
        selected: n.id === selectedNodeId,
        onSelect: handleSelect,
      },
    }));
    return { nodes: enriched, edges: laid.edges };
  }, [graph, selectedNodeId, handleSelect]);

  if (graph.nodes.length === 0) {
    return (
      <div className={styles.empty} role="status">
        No graph data — ingest a session first.
      </div>
    );
  }

  return (
    <div className={styles.wrap}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        fitView
        proOptions={{ hideAttribution: true }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
      >
        <Background />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}

export function CausalGraph(props: CausalGraphProps) {
  // Provider is required by @xyflow/react for hooks inside ReactFlow.
  return (
    <ReactFlowProvider>
      <CausalGraphInner {...props} />
    </ReactFlowProvider>
  );
}
