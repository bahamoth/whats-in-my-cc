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

type FileEventRecord = {
  file?: {
    path?: string;
    change_type?: string;
    old_path?: string;
    size_bytes?: number;
    observed_at?: string;
  };
};

type GitCommitRecord = {
  git?: {
    repo?: string;
    sha?: string;
    parents?: string[];
    author?: { name?: string; email?: string; time?: string };
    committer?: { name?: string; email?: string; time?: string };
    message?: string;
    branch?: string;
    files_changed?: string[];
  };
};

type DiffHunkRecord = {
  hunk?: {
    diff_hunk_id?: string;
    file_path?: string;
    change_type?: string;
    line_range_after?: { start?: number; end?: number } | null;
    introduced_by_commit_sha?: string;
    patch_preview?: string;
    lines_added?: number;
    lines_removed?: number;
  };
};

function FileEventSection({ record }: { record: unknown }) {
  if (typeof record !== 'object' || record === null) return null;
  const r = record as FileEventRecord;
  const f = r.file;
  if (!f) return null;
  return (
    <section className={styles.attributes} aria-labelledby="file-section-heading">
      <h4 id="file-section-heading">{f.change_type ?? 'file_event'}</h4>
      <table>
        <tbody>
          {f.path && (
            <tr><td className={styles.attrKey}>path</td><td className={styles.attrValue}>{f.path}</td></tr>
          )}
          {f.old_path && (
            <tr><td className={styles.attrKey}>old_path</td><td className={styles.attrValue}>{f.old_path}</td></tr>
          )}
          {typeof f.size_bytes === 'number' && (
            <tr><td className={styles.attrKey}>size_bytes</td><td className={styles.attrValue}>{f.size_bytes}</td></tr>
          )}
          {f.observed_at && (
            <tr><td className={styles.attrKey}>observed_at</td><td className={styles.attrValue}>{f.observed_at}</td></tr>
          )}
        </tbody>
      </table>
    </section>
  );
}

function GitCommitSection({ record }: { record: unknown }) {
  if (typeof record !== 'object' || record === null) return null;
  const r = record as GitCommitRecord;
  const g = r.git;
  if (!g) return null;
  const shortSha = g.sha?.slice(0, 7);
  const subject = (g.message ?? '').split('\n')[0].slice(0, 80);
  return (
    <section className={styles.attributes} aria-labelledby="git-section-heading">
      <h4 id="git-section-heading">{shortSha ?? 'git_commit'}{g.branch ? ` @ ${g.branch}` : ''}</h4>
      {subject && <p>{subject}</p>}
      <table>
        <tbody>
          {g.author?.name && (
            <tr><td className={styles.attrKey}>author</td><td className={styles.attrValue}>{g.author.name}</td></tr>
          )}
          {g.committer?.time && (
            <tr><td className={styles.attrKey}>committed_at</td><td className={styles.attrValue}>{g.committer.time}</td></tr>
          )}
          {g.repo && (
            <tr><td className={styles.attrKey}>repo</td><td className={styles.attrValue}>{g.repo}</td></tr>
          )}
        </tbody>
      </table>
      {g.files_changed && g.files_changed.length > 0 && (
        <details>
          <summary>files_changed ({g.files_changed.length})</summary>
          <ul>
            {g.files_changed.map((f) => <li key={f}>{f}</li>)}
          </ul>
        </details>
      )}
    </section>
  );
}

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
          {h.introduced_by_commit_sha && (
            <tr><td className={styles.attrKey}>commit</td><td className={styles.attrValue}>{h.introduced_by_commit_sha.slice(0, 7)}</td></tr>
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

type Props = { eventId: string | null };

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ok'; data: RawEventResponse }
  | { kind: 'error'; status: number; message: string };

export function SourcePanel({ eventId }: Props) {
  const [state, setState] = useState<State>(eventId ? { kind: 'loading' } : { kind: 'idle' });

  useEffect(() => {
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
  }, [eventId]);

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
          {state.data.record_type === 'file_event' && (
            <FileEventSection record={state.data.record} />
          )}
          {state.data.record_type === 'git_commit' && (
            <GitCommitSection record={state.data.record} />
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
