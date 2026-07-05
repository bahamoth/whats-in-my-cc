import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MetaStrip } from '../MetaStrip';
import type { SessionDetail } from '../../api/types';

const session: SessionDetail = {
  session_id: 's1',
  summary: {
    event_count: 42,
    by_kind: {},
    first_observed_at: '2026-07-03T00:00:00Z',
    last_observed_at: '2026-07-05T03:00:00Z',
  },
};

describe('MetaStrip', () => {
  it('shows the session span so a long (2-day+) session reads at a glance', () => {
    render(<MetaStrip session={session} events={[]} />);
    // span = last − first = 2d 3h (2026-07-05 세션 길이 정보 제공)
    expect(screen.getByText('2d 3h')).toBeTruthy();
    expect(screen.getByText(/42 events/)).toBeTruthy();
  });
});
