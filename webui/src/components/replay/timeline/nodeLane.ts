// webui/src/components/replay/timeline/nodeLane.ts
import { laneForNodeKind, type Lane, LANES } from '../../../api/laneMapping';

export function laneOfNodeKind(kind: string): Lane | null {
  return laneForNodeKind(kind);
}

/** Lanes (in canonical LANES order) that contain at least one of `nodes`.
 *  Used to hide empty lanes so each visible row gets more vertical room (#4). */
export function nonEmptyLanes<T extends { node_kind: string }>(nodes: T[]): Lane[] {
  const present = nodesByLane(nodes);
  return LANES.filter((l) => (present.get(l)?.length ?? 0) > 0);
}

export function nodesByLane<T extends { node_kind: string }>(nodes: T[]): Map<Lane, T[]> {
  const map = new Map<Lane, T[]>();
  for (const node of nodes) {
    const lane = laneForNodeKind(node.node_kind);
    if (lane !== null) {
      const bucket = map.get(lane);
      if (bucket) {
        bucket.push(node);
      } else {
        map.set(lane, [node]);
      }
    }
  }
  return map;
}
