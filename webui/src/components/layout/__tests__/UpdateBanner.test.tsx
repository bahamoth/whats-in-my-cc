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

  it('닫기 버튼으로 배너를 숨긴다', async () => {
    mockGet.mockResolvedValue({ current: '1.3.0', latest: 'v9.9.9', update_available: true });
    renderBanner();
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    await userEvent.click(screen.getByRole('button'));
    expect(screen.queryByRole('status')).toBeNull();
  });
});
