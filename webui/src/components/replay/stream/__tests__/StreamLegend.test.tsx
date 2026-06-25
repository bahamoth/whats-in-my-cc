import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, fireEvent, cleanup } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import '@testing-library/jest-dom/vitest';
import { StreamLegend } from '../StreamLegend';

describe('StreamLegend (controlled)', () => {
  beforeEach(() => {
    cleanup();
  });

  it('renders the lane / duration-heat key and the keyboard hints when open', () => {
    render(<StreamLegend open onClose={() => {}} />);
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

  it('renders nothing when closed — the dismissed state costs zero layout', () => {
    const { container } = render(<StreamLegend open={false} onClose={() => {}} />);
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText(/사용자/)).toBeNull();
  });

  it('the close button reports up via onClose (visibility owned by the page)', () => {
    const onClose = vi.fn();
    render(<StreamLegend open onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: /범례 닫기|close legend/i }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
