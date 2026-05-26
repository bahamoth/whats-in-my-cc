import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { SourcePanel } from '../SourcePanel';

describe('SourcePanel', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); cleanup(); });

  function envelope(data: unknown) {
    return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('shows empty hint when no event_id is selected', () => {
    render(<SourcePanel eventId={null} />);
    expect(screen.getByText(/click a node/i)).toBeInTheDocument();
  });

  it('renders diff_hunk node payload inline without fetching (slice-10a)', () => {
    // When the parent passes a diff_hunk graph node, SourcePanel must read
    // node.payload.hunk directly — no /v1/events/.../raw round-trip. The
    // diff_hunk side-table data already rides along on the graph response.
    const node = {
      node_id: 'nd_dh',
      schema_version: '0.5.0',
      session_id: 's_real',
      node_kind: 'diff_hunk',
      started_at: '2026-05-22T00:00:00Z',
      ended_at: null,
      merge_keys: { session_id: 's_real', diff_hunk_id: 'dh_inline' },
      source_event_ids: ['ev_intro_1'],
      source_uris: [],
      payload: {
        hunk: {
          diff_hunk_id: 'dh_inline',
          file_path: 'b.rs',
          change_type: 'modified',
          line_range_after: { start: 10, end: 12 },
          lines_added: 3,
          lines_removed: 2,
          introduced_by_event_id: 'ev_intro_1',
          introduced_by_tool_use_id: 'toolu_abc',
          patch_preview: '@@ inline diff @@',
          user_modified: false,
        },
      },
    };
    render(<SourcePanel eventId="ev_intro_1" node={node} />);
    expect(screen.getByText('b.rs L10-12')).toBeInTheDocument();
    expect(screen.getByText('toolu_abc')).toBeInTheDocument();
    expect((fetch as unknown as ReturnType<typeof vi.fn>)).not.toHaveBeenCalled();
  });

  it('fetches and renders raw record', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '1.0',
      event_id: 'ev_x',
      session_id: 's1',
      source: { kind: 'claude_transcript', file_path: '/tmp/a.jsonl', line_no: 42, ingested_at: 'now' },
      record: { type: 'user', content: 'hi' },
      record_type: 'user_message',
      redaction_state: 'none',
    }));
    render(<SourcePanel eventId="ev_x" />);
    await waitFor(() => expect(screen.getByText('user_message')).toBeInTheDocument());
    expect(screen.getByText('/tmp/a.jsonl')).toBeInTheDocument();
    expect(screen.getByText(/:42/)).toBeInTheDocument();
  });

  it('renders 404 message', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"event nope not found"}', { status: 404 })
    );
    render(<SourcePanel eventId="nope" />);
    await waitFor(() => expect(screen.getByText(/raw record not available/i)).toBeInTheDocument());
  });

  it('renders hook header + tool_input section for pre_tool_use', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.3.0',
      event_id: 'ev_hk1',
      session_id: 'sess_HK',
      source: {
        kind: 'hook',
        file_path: 'hook://sess_HK/PreToolUse/toolu_01',
        line_no: 0,
        ingested_at: '2026-05-19T00:00:00Z',
      },
      record: {
        session_id: 'sess_HK',
        hook_event_name: 'PreToolUse',
        tool_name: 'Bash',
        tool_input: { command: 'ls' },
        tool_use_id: 'toolu_01',
      },
      record_type: 'hook_event',
      redaction_state: 'none',
    }));
    const { container } = render(<SourcePanel eventId="ev_hk1" />);
    await waitFor(() => expect(screen.getByText('PreToolUse')).toBeInTheDocument());
    // tool_name appears in the hook section
    expect(screen.getByText('Bash')).toBeInTheDocument();
    // tool_input <summary> is rendered (use container query to disambiguate from JsonView)
    const summaries = Array.from(container.querySelectorAll('summary'));
    expect(summaries.some((s) => s.textContent === 'tool_input')).toBe(true);
  });

  it('renders hook header + message text for notification', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.3.0',
      event_id: 'ev_hk2',
      session_id: 'sess_HK',
      source: {
        kind: 'hook',
        file_path: 'hook://sess_HK/Notification/',
        line_no: 0,
        ingested_at: '2026-05-19T00:00:00Z',
      },
      record: {
        session_id: 'sess_HK',
        hook_event_name: 'Notification',
        message: 'msg-text-xyz',
      },
      record_type: 'hook_event',
      redaction_state: 'none',
    }));
    render(<SourcePanel eventId="ev_hk2" />);
    await waitFor(() => expect(screen.getByText('Notification')).toBeInTheDocument());
    expect(screen.getByText('msg-text-xyz')).toBeInTheDocument();
  });

  it('renders diff_hunk header with transcript-only attribution (slice-10a)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.5.0',
      event_id: 'ev_h1',
      session_id: 's_real',
      source: {
        kind: 'transcript',
        file_path:
          '~/.claude/projects/-foo/s_real.jsonl#diff_hunk=dh_1',
        line_no: 0,
        ingested_at: '2026-05-22T00:00:00Z',
      },
      record: {
        hunk: {
          diff_hunk_id: 'dh_1',
          file_path: 'a.rs',
          change_type: 'modified',
          line_range_after: { start: 42, end: 57 },
          introduced_by_event_id: 'ev_intro_1',
          introduced_by_tool_use_id: 'toolu_xyz',
          patch_preview: '@@ -40,3 +42,15 @@\n+added line\n',
          lines_added: 1,
          lines_removed: 0,
          user_modified: false,
        },
      },
      record_type: 'diff_hunk',
      redaction_state: 'none',
    }));
    const { container } = render(<SourcePanel eventId="ev_h1" />);
    await waitFor(() =>
      expect(screen.getByText('a.rs L42-57')).toBeInTheDocument()
    );
    expect(container.querySelector('pre')?.textContent).toContain('@@ -40,3 +42,15 @@');
    // Slice-10a — transcript-only attribution surfaces in the panel.
    expect(screen.getByText('toolu_xyz')).toBeInTheDocument();
    expect(screen.getByText('ev_intro_1')).toBeInTheDocument();
    // The dead `introduced_by_commit_sha` row must not be there any more.
    expect(screen.queryByText('commit')).toBeNull();
  });

  it('renders Attributes section when record_type is otel_span', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.2.0',
      event_id: 'ev_o',
      session_id: 'sess-otel-A',
      source: {
        kind: 'otel',
        file_path: 'otel://traces/abc/spans/def',
        line_no: 0,
        ingested_at: '2026-05-19T00:00:00Z',
      },
      record: {
        traceId: '5b8aa5a2d2c872e8321cf37308d69df2',
        spanId: '051581bf3cb55c13',
        name: 'tool.invoke',
      },
      record_type: 'otel_span',
      redaction_state: 'none',
      telemetry: {
        span_name: 'tool.invoke',
        span_kind: 'client',
        status_code: 'ok',
        attributes: { 'tool.name': 'Bash', 'session.id': 'sess-otel-A' },
      },
    }));
    render(<SourcePanel eventId="ev_o" />);
    await waitFor(() => expect(screen.getByText('Attributes')).toBeInTheDocument());
    expect(screen.getByText('tool.name')).toBeInTheDocument();
    expect(screen.getByText('Bash')).toBeInTheDocument();
  });
});
