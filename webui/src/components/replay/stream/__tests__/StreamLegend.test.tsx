import { describe, expect, it, beforeEach } from 'vitest';
import { screen, fireEvent, cleanup } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import '@testing-library/jest-dom/vitest';
import { StreamLegend } from '../StreamLegend';

describe('StreamLegend', () => {
  beforeEach(() => {
    cleanup();
    localStorage.clear();
  });

  it('renders the lane / duration-heat key and the keyboard hints', () => {
    render(<StreamLegend />);
    // lane colours
    expect(screen.getByText(/사용자/)).toBeInTheDocument();
    expect(screen.getByText(/배치/)).toBeInTheDocument();
    expect(screen.getByText(/워크플로우/)).toBeInTheDocument();
    // duration heat
    expect(screen.getByText(/10s/)).toBeInTheDocument();
    // keyboard hints (the j/k keys live in separate <kbd> tags; assert the
    // contiguous label text instead)
    expect(screen.getByText(/다음 오류/)).toBeInTheDocument();
  });

  it('dismisses on the close button and stays dismissed (persisted)', () => {
    render(<StreamLegend />);
    fireEvent.click(screen.getByRole('button', { name: /범례 닫기|dismiss/i }));
    expect(screen.queryByText(/사용자/)).toBeNull();

    // a fresh mount honours the persisted dismissal
    cleanup();
    render(<StreamLegend />);
    expect(screen.queryByText(/사용자/)).toBeNull();
  });
});
