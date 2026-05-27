import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { FindingsPanel } from '../FindingsPanel';

describe('FindingsPanel (slice-11)', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); cleanup(); });

  function envelope(data: unknown) {
    return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('renders empty state when no findings', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      envelope({ findings: [] }),
    );
    render(<FindingsPanel sessionId="s_empty" onSelectNode={() => {}} />);
    await waitFor(() =>
      expect(screen.getByText(/no findings/i)).toBeInTheDocument(),
    );
  });

  it('renders a tool_failure finding row with category, severity, claim, confidence', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      envelope({
        findings: [
          {
            finding_id: 'find_abc',
            schema_version: 'finding.v1',
            session_id: 's_real',
            category: 'tool_failure',
            severity: 'medium',
            claim: 'A tool result reported an error (is_error=true).',
            confidence: 0.95,
            limitations: ['User-rejection vs tool-error not distinguished in this rule version.'],
            evidence_refs: [{ node_id: 'nd_call_1', role: 'supporting' }],
            generated_at: '2026-05-26T10:00:00Z',
            rule_version: 'tool_failure.v1',
          },
        ],
      }),
    );
    render(<FindingsPanel sessionId="s_real" onSelectNode={() => {}} />);
    await waitFor(() =>
      expect(screen.getByText('tool_failure')).toBeInTheDocument(),
    );
    expect(screen.getByText('medium')).toBeInTheDocument();
    expect(screen.getByText(/reported an error/)).toBeInTheDocument();
    // confidence rendered as percentage
    expect(screen.getByText(/95\s*%/)).toBeInTheDocument();
  });

  it('clicking "show evidence" invokes onSelectNode with the evidence node_id', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      envelope({
        findings: [
          {
            finding_id: 'find_xyz',
            schema_version: 'finding.v1',
            session_id: 's_real',
            category: 'tool_failure',
            severity: 'medium',
            claim: 'x',
            confidence: 0.95,
            limitations: [],
            evidence_refs: [{ node_id: 'nd_target', role: 'supporting' }],
            generated_at: '2026-05-26T10:00:00Z',
            rule_version: 'tool_failure.v1',
          },
        ],
      }),
    );
    const onSelectNode = vi.fn();
    render(<FindingsPanel sessionId="s_real" onSelectNode={onSelectNode} />);
    const btn = await screen.findByRole('button', { name: /show evidence/i });
    fireEvent.click(btn);
    expect(onSelectNode).toHaveBeenCalledWith('nd_target');
  });
});
