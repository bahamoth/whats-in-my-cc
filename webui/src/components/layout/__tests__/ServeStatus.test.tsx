import React from 'react';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { ServeStatus } from '../ServeStatus';
import { I18nProvider } from '../../../i18n';

// growth-2026-07-18 — 사용자 요청: 버전·업데이트 여부·DB 크기를 웹에서
// 대략적으로 확인할 수 있는 UX. /v1/health의 version·db·retention 블록 소비.

function withI18n(node: React.ReactNode) {
  return <I18nProvider initialLocale="en">{node}</I18nProvider>;
}

const HEALTH = {
  status: 'ok',
  version: { current: '1.4.0', latest: 'v1.5.0', update_available: true },
  db: { path: '/data/wimcc.sqlite', size_bytes: 1_288_490_189, freelist_bytes: 4096 },
  security: { auth_required: false, retention_profile: 'default' },
  retention: {
    last_sweep_at: '2026-07-18T03:49:34+00:00',
    last_sweep_deletions: { raw_event: 12 },
  },
};

describe('ServeStatus', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('renders version and DB size from /v1/health', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(
      new Response(JSON.stringify(HEALTH), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    render(withI18n(<ServeStatus />));
    await waitFor(() => expect(screen.getByText(/v1\.4\.0/)).toBeInTheDocument());
    expect(screen.getByText(/1\.2\s?GiB/i)).toBeInTheDocument();
    // 업데이트 관측 사실 마커(판정 문장 없음) — update_available일 때만.
    expect(screen.getByTestId('update-marker')).toBeInTheDocument();
  });

  it('renders no marker when up to date, and nothing at all on fetch failure', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(
      new Response(
        JSON.stringify({
          ...HEALTH,
          version: { current: '1.4.0', latest: 'v1.4.0', update_available: false },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    );
    render(withI18n(<ServeStatus />));
    await waitFor(() => expect(screen.getByText(/v1\.4\.0/)).toBeInTheDocument());
    expect(screen.queryByTestId('update-marker')).toBeNull();

    cleanup();
    (fetch as unknown as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('down'));
    render(withI18n(<ServeStatus />));
    // 실패 시 조용히 생략(UpdateBanner와 동일한 fail-soft 계약).
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByText(/v1\.4\.0/)).toBeNull();
  });
});
