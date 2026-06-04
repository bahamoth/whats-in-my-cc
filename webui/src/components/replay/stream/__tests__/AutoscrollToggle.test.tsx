import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AutoscrollToggle } from '../AutoscrollToggle';

describe('AutoscrollToggle', () => {
  it('labels the control "자동 스크롤" and reflects ON via aria-pressed', () => {
    render(
      <AutoscrollToggle autoscroll={true} newCount={0} onEnable={() => {}} onDisable={() => {}} />,
    );
    const btn = screen.getByRole('button', { name: /자동 스크롤/ });
    expect(btn).toHaveAttribute('aria-pressed', 'true');
    expect(btn).toHaveTextContent('자동 스크롤');
  });

  it('toggles OFF (disable) when clicked while ON', () => {
    const onEnable = vi.fn();
    const onDisable = vi.fn();
    render(
      <AutoscrollToggle autoscroll={true} newCount={0} onEnable={onEnable} onDisable={onDisable} />,
    );
    fireEvent.click(screen.getByRole('button', { name: /자동 스크롤/ }));
    expect(onDisable).toHaveBeenCalledTimes(1);
    expect(onEnable).not.toHaveBeenCalled();
  });

  it('reflects OFF via aria-pressed and re-engages (enable) when clicked', () => {
    const onEnable = vi.fn();
    const onDisable = vi.fn();
    render(
      <AutoscrollToggle autoscroll={false} newCount={0} onEnable={onEnable} onDisable={onDisable} />,
    );
    const btn = screen.getByRole('button', { name: /자동 스크롤/ });
    expect(btn).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(btn);
    expect(onEnable).toHaveBeenCalledTimes(1);
    expect(onDisable).not.toHaveBeenCalled();
  });

  it('shows the new-message count when OFF and count > 0', () => {
    render(
      <AutoscrollToggle autoscroll={false} newCount={3} onEnable={() => {}} onDisable={() => {}} />,
    );
    expect(screen.getByTestId('autoscroll-new-count')).toHaveTextContent('3');
  });

  it('hides the count badge when count is 0', () => {
    render(
      <AutoscrollToggle autoscroll={false} newCount={0} onEnable={() => {}} onDisable={() => {}} />,
    );
    expect(screen.queryByTestId('autoscroll-new-count')).toBeNull();
  });

  it('hides the count badge while following (ON), even if count is stale > 0', () => {
    render(
      <AutoscrollToggle autoscroll={true} newCount={5} onEnable={() => {}} onDisable={() => {}} />,
    );
    expect(screen.queryByTestId('autoscroll-new-count')).toBeNull();
  });

  it('renders a leftSlot in the footer (e.g. the untagged-Bash control)', () => {
    render(
      <AutoscrollToggle
        autoscroll={true}
        newCount={0}
        onEnable={() => {}}
        onDisable={() => {}}
        leftSlot={<span data-testid="left-slot-content">extra</span>}
      />,
    );
    expect(screen.getByTestId('left-slot-content')).toBeInTheDocument();
  });
});
