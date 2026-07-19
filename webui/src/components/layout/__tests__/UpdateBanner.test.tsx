// Task 6 — the system banner that surfaces a new wimcc release. Consumes
// `getHealthVersion()` (health's raw `version` block, Task 4). Renders only
// when `update_available` is true; dismissible per session (no persistence —
// YAGNI). Follows the AppShell/LanguageToggle test convention: wrap in
// <I18nProvider> since useT() throws outside one.
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { I18nProvider } from '../../../i18n';
import { UpdateBanner } from '../UpdateBanner';

const mockGet = vi.hoisted(() => vi.fn());
vi.mock('../../../api/client', () => ({ getHealthVersion: mockGet }));

function renderBanner() {
  return render(
    <I18nProvider initialLocale="en">
      <UpdateBanner />
    </I18nProvider>,
  );
}

describe('UpdateBanner', () => {
  it('update_available=true면 두 버전 숫자를 표기한다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v9.9.9', update_available: true });
    renderBanner();
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain('v9.9.9');
      expect(screen.getByRole('status').textContent).toContain('v1.3.0');
    });
  });

  it('update_available=false면 렌더하지 않는다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v1.3.0', update_available: false });
    const { container } = renderBanner();
    await waitFor(() => expect(mockGet).toHaveBeenCalled());
    expect(container.firstChild).toBeNull();
  });

  // 2026-07-19 auto-update — 채널·다운로드 상태별 다음 행동 안내.
  it('downloaded가 있으면 재시작 적용 안내와 명령을 표기한다', async () => {
    mockGet.mockResolvedValue({
      current: '1.4.0',
      latest: 'v1.5.0',
      update_available: true,
      install_channel: 'shell',
      downloaded: 'v1.5.0',
    });
    renderBanner();
    await waitFor(() => {
      const text = screen.getByRole('status').textContent ?? '';
      expect(text).toContain('v1.5.0');
      expect(text).toMatch(/restart|재시작/i);
      expect(text).toContain('wimcc service restart');
    });
  });

  it('managed 채널이면 패키지 매니저 명령을 표기한다', async () => {
    mockGet.mockResolvedValue({
      current: '1.4.0',
      latest: 'v1.5.0',
      update_available: true,
      install_channel: 'managed',
      downloaded: null,
    });
    renderBanner();
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain(
        'brew upgrade bahamoth/tap/wimcc',
      );
    });
  });

  it('shell 채널(미다운로드)이면 self-update 명령을 표기한다', async () => {
    mockGet.mockResolvedValue({
      current: '1.4.0',
      latest: 'v1.5.0',
      update_available: true,
      install_channel: 'shell',
      downloaded: null,
    });
    renderBanner();
    await waitFor(() => {
      expect(screen.getByRole('status').textContent).toContain('wimcc self-update');
    });
  });

  it('닫기 버튼으로 배너를 숨긴다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v9.9.9', update_available: true });
    renderBanner();
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    await userEvent.click(screen.getByRole('button'));
    expect(screen.queryByRole('status')).toBeNull();
  });
});
