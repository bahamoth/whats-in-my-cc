// webui/src/components/replay/stream/BgGutter.tsx
// The unified left TIME-SPINE cell for ONE stream row (A+B redesign, 2026-06-14;
// see docs/superpowers/specs/2026-06-14-witmcc-ux-redesign-design.md §6.4).
//
// Every row draws a continuous main spine line (so adjacent rows read as ONE
// vertical time axis) plus a node colored by the row's primary kind. The
// background-subagent rails — formerly a separate right-anchored gutter — are
// ABSORBED into the spine: their lanes branch off it, packed just to the right
// of the main line. Width is constant regardless of concurrency: ≤3 agents pack
// as rails at a 4px pitch; ≥4 collapse to a single dense spine. Height follows
// the row (align-self stretch) so the virtualizer's measured row height is
// unchanged. Coverage is anchored at the subagent's own block row (see
// computeBgGutter): a 'start' rail runs from the node DOWN, an 'end' rail from
// the top UP to the node, 'mid' rows draw full height.
import type { GutterRow } from './streamModel';
import styles from './BgGutter.module.css';

/** A row's primary event kind — drives the spine node color. */
export type SpineKind =
  | 'user'
  | 'assistant'
  | 'tool'
  | 'thinking'
  | 'batch'
  | 'workflow'
  | 'scaffold';

const PITCH = 4; // px between background-subagent lane rails
const LANE_X0 = 13; // px inset of lane 0 from the LEFT edge (just right of the spine)

export function BgGutter({ row, kind }: { row: GutterRow | undefined; kind?: SpineKind | null }) {
  return (
    <div data-testid="gutter" className={styles.gutter} aria-hidden>
      {/* main time-spine: a continuous hairline every row → one vertical axis */}
      <div data-testid="spine-line" className={styles.spine} />
      {/* the row's node on the spine, colored by primary kind */}
      {kind && <div data-testid="spine-node" data-kind={kind} className={styles.spineNode} />}

      {row && row.dense > 0 && <div data-testid="gutter-dense" className={styles.dense} />}
      {row &&
        row.dense === 0 &&
        row.cells.map((c) => {
          const railClass =
            c.marker === 'start' ? styles.railStart : c.marker === 'end' ? styles.railEnd : styles.railMid;
          return (
            <div key={c.agentId} className={styles.lane} style={{ left: LANE_X0 + c.lane * PITCH }}>
              <div data-testid="gutter-rail" className={`${styles.rail} ${railClass}`} style={{ background: c.color }} />
              {c.marker === 'start' && (
                <div data-testid="gutter-start" className={styles.node} style={{ background: c.color }} />
              )}
              {c.marker === 'end' && (
                <div
                  data-testid="gutter-end"
                  className={`${styles.node} ${styles.nodeEnd}`}
                  style={{ background: c.color }}
                />
              )}
            </div>
          );
        })}
    </div>
  );
}
