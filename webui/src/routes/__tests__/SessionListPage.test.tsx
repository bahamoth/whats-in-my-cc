import React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import SessionListPage from '../SessionListPage';

function withRouter(node: React.ReactNode) {
  return <MemoryRouter>{node}</MemoryRouter>;
}

describe('SessionListPage', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); });

  function envelope(data: unknown) {
    return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('renders empty state with CLI hint', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([]));
    render(withRouter(<SessionListPage />));
    await waitFor(() => expect(screen.getByText(/no sessions yet/i)).toBeInTheDocument());
    expect(screen.getByText(/witmcc ingest --all/)).toBeInTheDocument();
  });

  it('renders rows sorted by last_observed_at desc', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 'older', first_observed_at: '2026-05-19T08:00:00Z', last_observed_at: '2026-05-19T09:00:00Z', event_count: 5, source_uris: [] },
      { session_id: 'newer', first_observed_at: '2026-05-19T10:00:00Z', last_observed_at: '2026-05-19T11:00:00Z', event_count: 7, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    const rows = await screen.findAllByRole('row');
    // [header, newer, older]
    expect(rows[1]).toHaveTextContent('newer');
    expect(rows[2]).toHaveTextContent('older');
  });

  it('renders error state with retry', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"db gone"}', { status: 500 })
    );
    render(withRouter(<SessionListPage />));
    await waitFor(() => expect(screen.getByText(/db gone/)).toBeInTheDocument());
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });
});
