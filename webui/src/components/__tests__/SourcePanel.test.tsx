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
