// webui/src/components/replay/detail/RawTab.tsx
import { useRef, useState } from 'react';
import { JsonTree } from './JsonTree';
import styles from './JsonTree.module.css';

export interface RawBlock {
  source: string;
  label: string;
  record: unknown;
}

interface RawTabProps {
  nodeId: string | null;
  record: unknown;
  /** When provided, renders each block as a source-labelled section.
   *  Falls back to single-record (back-compat) when absent or empty. */
  blocks?: RawBlock[];
}

export function RawTab({ nodeId, record, blocks }: RawTabProps) {
  // expansion sets keyed by block key; survives re-render and data refresh
  const store = useRef<Map<string, Set<string>>>(new Map());
  const [, force] = useState(0);

  // --- source-split block mode ---
  if (blocks && blocks.length > 0) {
    return (
      <div className={styles.blockList}>
        {blocks.map((block, i) => {
          const key = `${nodeId ?? '$anon'}:${block.source}:${i}`;
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
          return (
            <div key={key} className={styles.block}>
              <div className={styles.blockHeader}>
                <span className={styles.blockSource}>{block.source}</span>
                <span className={styles.blockLabel}>{block.label}</span>
              </div>
              <JsonTree data={block.record} expanded={set} onToggle={onToggle} />
            </div>
          );
        })}
      </div>
    );
  }

  // --- single-record back-compat mode ---
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
