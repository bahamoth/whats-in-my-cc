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

  it('renders file_event header with path + change_type (slice-5)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.4.0',
      event_id: 'ev_f1',
      session_id: 'filesystem',
      source: {
        kind: 'file_git',
        file_path: 'file:///tmp/a.rs',
        line_no: 0,
        ingested_at: '2026-05-20T00:00:00Z',
      },
      record: {
        file: {
          path: '/tmp/a.rs',
          change_type: 'modified',
          size_bytes: 42,
          observed_at: '2026-05-20T00:00:00Z',
        },
      },
      record_type: 'file_event',
      redaction_state: 'none',
    }));
    render(<SourcePanel eventId="ev_f1" />);
    await waitFor(() => expect(screen.getByText('modified')).toBeInTheDocument());
    expect(screen.getByText('/tmp/a.rs')).toBeInTheDocument();
  });

  it('renders git_commit header with short sha + branch + subject (slice-5)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.4.0',
      event_id: 'ev_g1',
      session_id: 'filesystem',
      source: {
        kind: 'file_git',
        file_path: 'git:///tmp/r/commit/abc1234def',
        line_no: 0,
        ingested_at: '2026-05-20T00:00:00Z',
      },
      record: {
        git: {
          repo: '/tmp/r',
          sha: 'abc1234def56789',
          branch: 'main',
          message: 'fix: bump version\n\nbody',
          author: { name: 'al', email: 'a@x', time: '2026-05-20T00:00:00Z' },
          committer: { name: 'al', email: 'a@x', time: '2026-05-20T00:00:00Z' },
          files_changed: ['a.rs', 'b.rs'],
        },
      },
      record_type: 'git_commit',
      redaction_state: 'none',
    }));
    render(<SourcePanel eventId="ev_g1" />);
    await waitFor(() => expect(screen.getByText(/abc1234 @ main/)).toBeInTheDocument());
    expect(screen.getByText('fix: bump version')).toBeInTheDocument();
    expect(screen.getByText(/files_changed \(2\)/)).toBeInTheDocument();
  });

  it('renders diff_hunk header with file_path + line range + patch preview (slice-5)', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '0.4.0',
      event_id: 'ev_h1',
      session_id: 'filesystem',
      source: {
        kind: 'file_git',
        file_path: 'git:///tmp/r/commit/abc/hunk/a.rs:42-57',
        line_no: 0,
        ingested_at: '2026-05-20T00:00:00Z',
      },
      record: {
        hunk: {
          diff_hunk_id: 'hunk_1',
          file_path: 'a.rs',
          change_type: 'modified',
          line_range_after: { start: 42, end: 57 },
          introduced_by_commit_sha: 'abc1234def',
          patch_preview: '@@ -40,3 +42,15 @@\n+added line\n',
          lines_added: 1,
          lines_removed: 0,
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
