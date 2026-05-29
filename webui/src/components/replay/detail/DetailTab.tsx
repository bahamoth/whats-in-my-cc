// webui/src/components/replay/detail/DetailTab.tsx
import type { GraphNodeDto } from '../../../api/types';
import styles from './DetailTab.module.css';

interface DetailTabProps {
  node: GraphNodeDto | null;
  record: unknown;
  episodePhase: string | null;
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function fmtTime(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().replace('T', ' ').slice(0, 19);
}

export function DetailTab({ node, record, episodePhase }: DetailTabProps) {
  if (!node) return <p className={styles.empty}>Select a node to see its details.</p>;

  const rec = asObj(record);
  const usage = asObj(asObj(rec.message).usage);
  const toolResult = asObj(rec.tool_result);
  const hasUsage = Object.keys(usage).length > 0;
  const isError = toolResult.is_error === true;

  const rows: Array<[string, string]> = [
    ['kind', node.node_kind],
    ['started', fmtTime(node.started_at)],
    ['ended', fmtTime(node.ended_at)],
  ];
  if (episodePhase) rows.push(['episode', episodePhase]);

  return (
    <div className={styles.detail}>
      <table className={styles.table}>
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <td className={styles.k}>{k}</td>
              <td className={styles.v}>{k === 'episode' ? <span className={styles.phase}>{v}</span> : v}</td>
            </tr>
          ))}
          {isError && (
            <tr><td className={styles.k}>result</td><td className={styles.v}><span className={styles.error}>error</span></td></tr>
          )}
        </tbody>
      </table>

      {hasUsage && (
        <div className={styles.usage} aria-label="token usage">
          <span className={styles.badge}>out {String(usage.output_tokens ?? '—')}</span>
          <span className={styles.badge}>in {String(usage.input_tokens ?? '—')}</span>
          {usage.cache_read_input_tokens != null && <span className={styles.badge}>cache {String(usage.cache_read_input_tokens)}</span>}
        </div>
      )}
    </div>
  );
}
