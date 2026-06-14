// webui/src/components/replay/stream/BgGutter.tsx
// Fixed-width (~14px) left gutter painting the background-subagent hairlines for
// ONE stream row. Width is CONSTANT regardless of concurrency: ≤3 agents pack as
// rails at a 4px pitch (lane 0 nearest the card); ≥4 collapse to a single
// neutral spine. Height follows the row (align-self stretch) so the virtualizer's
// measured row height is unchanged.
//
// Coverage is anchored at the subagent's own block row (see computeBgGutter):
// the rail STARTS on the subagent card and extends DOWN over the concurrent main
// rows — it never bleeds up onto an earlier main-thread row that merely overlaps
// in time. The rail is CLIPPED at its end nodes — it does not overshoot the ▢/◯:
// a 'start' row draws the rail from the node (card vertical center) DOWN; an
// 'end' row draws it from the top UP TO the node; 'mid' rows draw full height.
// Nodes sit on the rail's vertical center (no horizontal connector — the rail is
// already beside the card, so a connector only ever read as a misaligned tick).
import type { GutterRow } from './streamModel';
import styles from './BgGutter.module.css';

const PITCH = 4; // px between lane rails
const X0 = 3; // px inset of lane 0 from the card-facing (right) edge

export function BgGutter({ row }: { row: GutterRow | undefined }) {
  return (
    <div data-testid="gutter" className={styles.gutter} aria-hidden>
      {row && row.dense > 0 && <div data-testid="gutter-dense" className={styles.dense} />}
      {row &&
        row.dense === 0 &&
        row.cells.map((c) => {
          const railClass =
            c.marker === 'start' ? styles.railStart : c.marker === 'end' ? styles.railEnd : styles.railMid;
          return (
            <div key={c.agentId} className={styles.lane} style={{ right: X0 + c.lane * PITCH }}>
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
