// webui/src/components/replay/detail/RawTab.tsx
import { useRef, useState } from 'react';
import { JsonTree } from './JsonTree';
import styles from './JsonTree.module.css';

interface RawTabProps {
  nodeId: string | null;
  record: unknown;
}

export function RawTab({ nodeId, record }: RawTabProps) {
  // expansion sets keyed by node id; survives re-render and data refresh
  const store = useRef<Map<string, Set<string>>>(new Map());
  const [, force] = useState(0);

  if (record == null) {
    return <p className={styles.empty}>No raw record — select a node.</p>;
  }

  const key = nodeId ?? '$anon';
  let set = store.current.get(key);
  if (!set) {
    set = new Set<string>(['$']); // root open by default
    store.current.set(key, set);
  }

  const onToggle = (path: string) => {
    const s = store.current.get(key)!;
    if (s.has(path)) s.delete(path);
    else s.add(path);
    force((n) => n + 1);
  };

  return <JsonTree data={record} expanded={set} onToggle={onToggle} />;
}
