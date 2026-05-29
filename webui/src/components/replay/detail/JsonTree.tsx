// webui/src/components/replay/detail/JsonTree.tsx
import { ChevronRight, ChevronDown } from 'lucide-react';
import styles from './JsonTree.module.css';

export interface JsonTreeProps {
  data: unknown;
  /** Set of open paths. Root path is "$". */
  expanded: Set<string>;
  onToggle: (path: string) => void;
}

function isContainer(v: unknown): v is object {
  return v !== null && typeof v === 'object';
}

function formatPrimitive(v: unknown): string {
  if (typeof v === 'string') return `"${v}"`;
  if (v === null) return 'null';
  return String(v);
}

function Node({ k, value, path, expanded, onToggle }: { k: string | null; value: unknown; path: string; expanded: Set<string>; onToggle: (p: string) => void }) {
  const container = isContainer(value);
  const open = expanded.has(path);

  if (!container) {
    return (
      <div className={styles.row}>
        {k !== null && <span className={styles.key}>{k}</span>}
        {k !== null && <span className={styles.colon}>:</span>}
        <span className={styles.value}>{formatPrimitive(value)}</span>
      </div>
    );
  }

  const entries = Array.isArray(value)
    ? value.map((v, i) => [String(i), v] as const)
    : Object.entries(value as Record<string, unknown>);
  const label = k ?? '$';
  const Chevron = open ? ChevronDown : ChevronRight;

  return (
    <div className={styles.node}>
      <div className={styles.row}>
        <button type="button" className={styles.toggle} onClick={() => onToggle(path)} aria-expanded={open}>
          <Chevron size={12} aria-hidden />
          <span className={styles.key}>{label}</span>
          <span className={styles.preview}>{Array.isArray(value) ? `[${entries.length}]` : `{${entries.length}}`}</span>
        </button>
      </div>
      {open && (
        <div className={styles.children}>
          {entries.map(([childKey, childVal]) => (
            // Path is dot-joined. Limitation: a key containing "." could
            // collide with the path namespace (e.g. {"a.b":…} vs {a:{b:…}}).
            // Acceptable for Claude Code raw records, whose keys are dotless.
            <Node key={childKey} k={childKey} value={childVal} path={`${path}.${childKey}`} expanded={expanded} onToggle={onToggle} />
          ))}
        </div>
      )}
    </div>
  );
}

export function JsonTree({ data, expanded, onToggle }: JsonTreeProps) {
  return (
    <div className={styles.tree}>
      <Node k={null} value={isContainer(data) ? data : { value: data }} path="$" expanded={expanded} onToggle={onToggle} />
    </div>
  );
}
