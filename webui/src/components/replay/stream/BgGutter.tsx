// webui/src/components/replay/stream/BgGutter.tsx
// Fixed-width (~14px) left gutter painting the background-subagent hairlines for
// ONE stream row. Width is CONSTANT regardless of concurrency: ≤3 agents pack as
// rails at a 4px pitch; ≥4 collapse to a single neutral spine (the count rides
// the row's own bg-marker chip). Height follows the row (align-self stretch) so
// the virtualizer's measured row height is unchanged — no reflow / anchor risk.
import type { GutterRow } from './streamModel';
import styles from './BgGutter.module.css';

const PITCH = 4; // px between lane rails
const X0 = 3; // px left inset of lane 0

export function BgGutter({ row }: { row: GutterRow | undefined }) {
  return (
    <div data-testid="gutter" className={styles.gutter} aria-hidden>
      {row && row.dense > 0 && <div data-testid="gutter-dense" className={styles.dense} />}
      {row &&
        row.dense === 0 &&
        row.cells.map((c) => {
          const left = X0 + c.lane * PITCH;
          return (
            <div key={c.agentId} className={styles.lane} style={{ left }}>
              <div data-testid="gutter-rail" className={styles.rail} style={{ background: c.color }} />
              {c.marker === 'start' && (
                <div
                  data-testid="gutter-start"
                  className={styles.start}
                  style={{ boxShadow: `0 0 0 1.6px ${c.color}` }}
                />
              )}
              {c.marker === 'end' && (
                <div data-testid="gutter-end" className={styles.end} style={{ background: c.color }} />
              )}
            </div>
          );
        })}
    </div>
  );
}
