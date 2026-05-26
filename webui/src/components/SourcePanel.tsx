import { useEffect, useState } from 'react';
import { ApiError, getEventRaw } from '../api/client';
import type { RawEventResponse } from '../api/types';
import { JsonView } from './JsonView';
import styles from './SourcePanel.module.css';

type HookRecord = {
  hook_event_name?: string;
  tool_name?: string;
  tool_use_id?: string;
  tool_input?: unknown;
  tool_response?: unknown;
  prompt?: string;
  message?: string;
  trigger?: string;
  source?: string;
};

// Slice-10a — transcript-only diff_hunk record. The previous
// `introduced_by_commit_sha` field is gone with the git poller.
type DiffHunkRecord = {
  hunk?: {
    diff_hunk_id?: string;
    file_path?: string;
    change_type?: string;
    line_range_after?: { start?: number; end?: number } | null;
    introduced_by_event_id?: string;
    introduced_by_tool_use_id?: string | null;
    patch_preview?: string;
    lines_added?: number;
    lines_removed?: number;
    user_modified?: boolean;
  };
};

function DiffHunkSection({ record }: { record: unknown }) {
  if (typeof record !== 'object' || record === null) return null;
  const r = record as DiffHunkRecord;
  const h = r.hunk;
  if (!h) return null;
  const range = h.line_range_after
    ? `${h.line_range_after.start}-${h.line_range_after.end}`
    : 'binary';
  return (
    <section className={styles.attributes} aria-labelledby="hunk-section-heading">
      <h4 id="hunk-section-heading">{h.file_path ?? 'diff_hunk'} L{range}</h4>
      <table>
        <tbody>
          {h.change_type && (
            <tr><td className={styles.attrKey}>change_type</td><td className={styles.attrValue}>{h.change_type}</td></tr>
          )}
          {typeof h.lines_added === 'number' && (
            <tr><td className={styles.attrKey}>+ lines</td><td className={styles.attrValue}>{h.lines_added}</td></tr>
          )}
          {typeof h.lines_removed === 'number' && (
            <tr><td className={styles.attrKey}>- lines</td><td className={styles.attrValue}>{h.lines_removed}</td></tr>
          )}
          {h.introduced_by_tool_use_id && (
            <tr><td className={styles.attrKey}>tool_use_id</td><td className={styles.attrValue}>{h.introduced_by_tool_use_id}</td></tr>
          )}
          {h.introduced_by_event_id && (
            <tr><td className={styles.attrKey}>event_id</td><td className={styles.attrValue}>{h.introduced_by_event_id}</td></tr>
          )}
          {h.user_modified && (
            <tr><td className={styles.attrKey}>user_modified</td><td className={styles.attrValue}>true</td></tr>
          )}
        </tbody>
      </table>
      {h.patch_preview && (
        <pre>{h.patch_preview}</pre>
      )}
    </section>
  );
}

function HookSection({ record }: { record: unknown }) {
  if (typeof record !== 'object' || record === null) return null;
  const r = record as HookRecord;
  if (!r.hook_event_name) return null;
  return (
    <section className={styles.attributes} aria-labelledby="hook-section-heading">
      <h4 id="hook-section-heading">{r.hook_event_name}</h4>
      <table>
        <tbody>
          {r.tool_name && (
            <tr><td className={styles.attrKey}>tool_name</td><td className={styles.attrValue}>{r.tool_name}</td></tr>
          )}
          {r.tool_use_id && (
            <tr><td className={styles.attrKey}>tool_use_id</td><td className={styles.attrValue}>{r.tool_use_id}</td></tr>
          )}
          {r.trigger && (
            <tr><td className={styles.attrKey}>trigger</td><td className={styles.attrValue}>{r.trigger}</td></tr>
          )}
          {r.source && (
            <tr><td className={styles.attrKey}>source</td><td className={styles.attrValue}>{r.source}</td></tr>
          )}
        </tbody>
      </table>
      {r.tool_input !== undefined && (
        <details open>
          <summary>tool_input</summary>
          <JsonView data={r.tool_input} />
        </details>
      )}
      {r.tool_response !== undefined && (
        <details>
          <summary>tool_response</summary>
          <JsonView data={r.tool_response} />
        </details>
      )}
      {r.prompt && <pre>{r.prompt}</pre>}
      {r.message && <p>{r.message}</p>}
    </section>
  );
}

// Slice-10a — `node` lets callers short-circuit the raw_event round-trip for
// node kinds whose payload already rides on the graph response (currently
// just diff_hunk). For everything else, eventId is used and the existing
// fetch flow is unchanged.
type Props = {
  eventId: string | null;
  node?: {
    node_kind: string;
    payload: unknown;
  } | null;
};

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ok'; data: RawEventResponse }
  | { kind: 'error'; status: number; message: string };

function isDiffHunkNode(node: Props['node']): node is { node_kind: 'diff_hunk'; payload: { hunk: unknown } } {
  if (!node || node.node_kind !== 'diff_hunk') return false;
  const p = node.payload as { hunk?: unknown } | null;
  return !!(p && typeof p === 'object' && p.hunk);
}

export function SourcePanel({ eventId, node }: Props) {
  const skipFetch = isDiffHunkNode(node);
  const [state, setState] = useState<State>(
    eventId && !skipFetch ? { kind: 'loading' } : { kind: 'idle' },
  );

  useEffect(() => {
    if (skipFetch) {
      setState({ kind: 'idle' });
      return;
    }
    if (!eventId) { setState({ kind: 'idle' }); return; }
    let cancelled = false;
    setState({ kind: 'loading' });
    getEventRaw(eventId)
      .then((data) => { if (!cancelled) setState({ kind: 'ok', data }); })
      .catch((e: unknown) => {
        if (cancelled) return;
        if (e instanceof ApiError) setState({ kind: 'error', status: e.status, message: e.detail });
        else setState({ kind: 'error', status: 0, message: String(e) });
      });
    return () => { cancelled = true; };
  }, [eventId, skipFetch]);

  // Inline render path for diff_hunk: graph already carries everything we
  // need. The DiffHunkSection + JsonView read the node payload directly.
  if (skipFetch && node) {
    return (
      <aside className={styles.panel}>
        <DiffHunkSection record={node.payload} />
        <div className={styles.body}>
          <JsonView data={node.payload} />
        </div>
      </aside>
    );
  }

  return (
    <aside className={styles.panel}>
      {state.kind === 'idle' && <p className={styles.hint}>Click a node to see its source record.</p>}
      {state.kind === 'loading' && <p>Loading raw record…</p>}
      {state.kind === 'error' && state.status === 404 && (
        <p className={styles.hint}>raw record not available for this event</p>
      )}
      {state.kind === 'error' && state.status === 410 && (
        <p className={styles.hint}>raw record pruned by retention</p>
      )}
      {state.kind === 'error' && state.status !== 404 && state.status !== 410 && (
        <p role="alert">Error: {state.message}</p>
      )}
      {state.kind === 'ok' && (
        <>
          <header className={styles.header}>
            <span className={styles.type}>{state.data.record_type}</span>
            <span className={styles.source}>
              <span>{state.data.source.file_path}</span>
              <span>:{state.data.source.line_no}</span>
            </span>
          </header>
          {state.data.record_type === 'otel_span' && state.data.telemetry?.attributes && (
            <section className={styles.attributes} aria-labelledby="otel-attrs-heading">
              <h4 id="otel-attrs-heading">Attributes</h4>
              <table>
                <tbody>
                  {Object.entries(state.data.telemetry.attributes).map(([k, v]) => (
                    <tr key={k}>
                      <td className={styles.attrKey}>{k}</td>
                      <td className={styles.attrValue}>{String(v)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          {state.data.record_type === 'hook_event' && (
            <HookSection record={state.data.record} />
          )}
          {state.data.record_type === 'diff_hunk' && (
            <DiffHunkSection record={state.data.record} />
          )}
          <div className={styles.body}>
            <JsonView data={state.data.record} />
          </div>
        </>
      )}
    </aside>
  );
}
