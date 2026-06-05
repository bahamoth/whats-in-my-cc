// webui/src/components/replay/detail/NodeDetail.tsx
// no longer used by InsightTab (metrics-led redesign)
import type { GraphNodeDto, FindingDto } from '../../../api/types';
import { nodeLabel } from '../stream/nodeLabel';
import styles from './NodeDetail.module.css';

interface NodeDetailProps {
  node: GraphNodeDto;
  record: unknown;
  findings: FindingDto[];
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function fmtTime(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().replace('T', ' ').slice(0, 19);
}

const KIND_ICON: Record<string, string> = {
  tool: '⚙',
  assistant: '✦',
  thinking: '…',
  user: '◎',
  hook: '↩',
  span: '◇',
  verify: '✓',
  diff: '±',
  other: '·',
};

const SEV_CLASS: Record<string, string> = { high: 'sevHigh', medium: 'sevMed', low: 'sevLow' };

const LONG_THRESHOLD = 60;

function isLong(val: string): boolean {
  return val.length > LONG_THRESHOLD || val.includes('\n');
}

function isFilePath(key: string, val: string): boolean {
  return (key === 'file_path' || key === 'path') && val.startsWith('/');
}

function ParamValue({ paramKey, val }: { paramKey: string; val: string }) {
  if (isLong(val)) {
    return <pre className={styles.paramPre}>{val}</pre>;
  }
  if (isFilePath(paramKey, val)) {
    return <span className={`${styles.paramVal}`} style={{ fontFamily: 'var(--wimcc-mono, ui-monospace, monospace)' }}>{val}</span>;
  }
  return <span className={styles.paramVal}>{val}</span>;
}

function ToolCallSection({ payload, record }: { payload: Record<string, unknown>; record: unknown }) {
  const input = asObj(payload.input);
  const toolResult = asObj(asObj(record).tool_result);
  const hasResult = record != null && 'tool_result' in asObj(record);
  const isError = toolResult.is_error === true;

  return (
    <>
      <div className={styles.paramList}>
        {Object.entries(input).map(([k, v]) => {
          const strVal = typeof v === 'string' ? v : JSON.stringify(v, null, 2) ?? String(v);
          return (
            <div key={k} className={styles.paramRow}>
              <span className={styles.paramKey}>{k}</span>
              <ParamValue paramKey={k} val={strVal} />
            </div>
          );
        })}
      </div>
      {hasResult && (
        <div className={styles.resultRow}>
          {isError
            ? <span className={styles.badgeError}>error</span>
            : <span className={styles.badgeOk}>ok</span>}
        </div>
      )}
    </>
  );
}

function AssistantSection({ payload, record }: { payload: Record<string, unknown>; record: unknown }) {
  const text = typeof payload.text === 'string' ? payload.text : '';
  const usage = asObj(asObj(asObj(record).message).usage);
  const hasUsage = Object.keys(usage).length > 0;

  return (
    <>
      <p className={styles.messageText}>{text}</p>
      {hasUsage && (
        <div className={styles.usage} aria-label="token usage">
          <span className={styles.badge}>out {String(usage.output_tokens ?? '—')}</span>
          <span className={styles.badge}>in {String(usage.input_tokens ?? '—')}</span>
          {usage.cache_read_input_tokens != null && (
            <span className={styles.badge}>cache {String(usage.cache_read_input_tokens)}</span>
          )}
        </div>
      )}
    </>
  );
}

function HookSection({ payload }: { payload: Record<string, unknown> }) {
  const hookName =
    (payload.hookName as string | undefined) ??
    (asObj(payload.hook).hook_event_name as string | undefined) ??
    '';
  const exitCode = payload.exitCode ?? payload.exit_code;
  const stdout = typeof payload.stdout === 'string' ? payload.stdout : null;

  return (
    <>
      <span className={styles.hookName}>{hookName || '—'}</span>
      {exitCode != null && (
        <div style={{ marginTop: 6 }}>
          <span className={styles.exitCode}>exit {String(exitCode)}</span>
        </div>
      )}
      {stdout && (
        <pre className={styles.stdout}>{stdout}</pre>
      )}
    </>
  );
}

function OtelSection({ payload }: { payload: Record<string, unknown> }) {
  const rawSpan = asObj(payload.raw_span);
  const name = typeof rawSpan.name === 'string' ? rawSpan.name : '—';
  const startNs = typeof rawSpan.start_time_unix_nano === 'number' ? rawSpan.start_time_unix_nano : null;
  const endNs = typeof rawSpan.end_time_unix_nano === 'number' ? rawSpan.end_time_unix_nano : null;
  const durationMs = startNs != null && endNs != null ? ((endNs - startNs) / 1_000_000).toFixed(1) : null;

  return (
    <>
      <span className={styles.paramVal}>{name}</span>
      {durationMs != null && (
        <div style={{ marginTop: 6 }}>
          <span className={styles.badge}>{durationMs} ms</span>
        </div>
      )}
    </>
  );
}

function KindSection({ node, record }: { node: GraphNodeDto; record: unknown }) {
  const payload = asObj(node.payload);
  switch (node.node_kind) {
    case 'tool_call':
      return <ToolCallSection payload={payload} record={record} />;
    case 'assistant_message':
    case 'thinking':
      return <AssistantSection payload={payload} record={record} />;
    case 'hook_event':
      return <HookSection payload={payload} />;
    case 'otel_span':
      return <OtelSection payload={payload} />;
    default:
      return null;
  }
}

export function NodeDetail({ node, record, findings }: NodeDetailProps) {
  const label = nodeLabel(node);
  const icon = KIND_ICON[label.kind] ?? KIND_ICON.other;

  const rows: Array<[string, string]> = [
    ['started', fmtTime(node.started_at)],
    ['ended', fmtTime(node.ended_at)],
  ];

  return (
    <div className={styles.root}>
      {/* Header */}
      <div className={styles.header}>
        <span className={styles.icon} aria-hidden="true">{icon}</span>
        <span className={styles.primary}>{label.primary}</span>
        <span className={styles.nodeId}>{node.node_id}</span>
      </div>

      {/* Common rows */}
      <table className={styles.table}>
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <td className={styles.k}>{k}</td>
              <td className={styles.v}>{v}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {/* Per-kind section */}
      <div className={styles.section}>
        <KindSection node={node} record={record} />
      </div>

      {/* Findings */}
      {findings.length > 0 && (
        <div className={styles.section}>
          <div className={styles.sectionTitle}>Findings</div>
          <ul className={styles.findingsList}>
            {findings.map((f) => (
              <li key={f.finding_id} className={styles.findingItem}>
                <div className={styles.findingHead}>
                  <span className={`${styles.sev} ${styles[SEV_CLASS[f.severity] ?? 'sevLow']}`}>
                    {f.severity}
                  </span>
                  <span className={styles.findingCategory}>{f.category}</span>
                  <span className={styles.findingConf}>{Math.round(f.confidence * 100)}%</span>
                </div>
                <p className={styles.findingSummary}>{f.summary}</p>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
