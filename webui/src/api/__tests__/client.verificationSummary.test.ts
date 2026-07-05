import { afterEach, describe, expect, it, vi } from 'vitest';
import { getVerificationSummary } from '../client';

describe('getVerificationSummary', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('session_id를 쿼리 파라미터로 전달한다', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ meta: { generated_at: 'x' }, data: { total: 0 } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetchMock);
    await getVerificationSummary({ session_id: 'sess_1' });
    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).toContain('/v1/verification/summary?session_id=sess_1');
  });
});
