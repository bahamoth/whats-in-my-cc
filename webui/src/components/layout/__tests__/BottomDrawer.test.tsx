/**
 * PR-5 RED — BottomDrawer is the slide-up panel for raw JSON deep-dive.
 *
 * Locked behaviour:
 *  - When `open={false}`, NO children render — DOM cost is zero. This is
 *    a load-time guarantee for the raw JSON tree, which can be heavy.
 *  - When `open={true}`, children render inside `role="dialog"`
 *    with `aria-modal="true"` and `aria-labelled-by` pointing at the
 *    drawer's title.
 *  - `onClose` fires on backdrop click and on Escape keypress.
 *  - Drawer has a recognisable testid `bottom-drawer` and the open state
 *    is reflected in `data-state="open|closed"`.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent, screen } from '@testing-library/react';
import { BottomDrawer } from '../BottomDrawer';

describe('BottomDrawer', () => {
  it('does not render children when closed', () => {
    render(
      <BottomDrawer open={false} onClose={vi.fn()} title="Raw">
        <p>HEAVY_CONTENT</p>
      </BottomDrawer>,
    );
    expect(screen.queryByText('HEAVY_CONTENT')).toBeNull();
  });

  it('renders children inside a dialog when open', () => {
    render(
      <BottomDrawer open onClose={vi.fn()} title="Raw event">
        <p>HEAVY_CONTENT</p>
      </BottomDrawer>,
    );
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(screen.getByText('HEAVY_CONTENT')).toBeInTheDocument();
  });

  it('exposes the title via aria-labelledby', () => {
    render(
      <BottomDrawer open onClose={vi.fn()} title="My Title">
        <p>x</p>
      </BottomDrawer>,
    );
    const dialog = screen.getByRole('dialog');
    const id = dialog.getAttribute('aria-labelledby');
    expect(id).toBeTruthy();
    expect(document.getElementById(id!)?.textContent).toBe('My Title');
  });

  it('calls onClose on Escape keypress', () => {
    const onClose = vi.fn();
    render(
      <BottomDrawer open onClose={onClose} title="Raw">
        <p>x</p>
      </BottomDrawer>,
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('calls onClose on backdrop click', () => {
    const onClose = vi.fn();
    render(
      <BottomDrawer open onClose={onClose} title="Raw">
        <p>x</p>
      </BottomDrawer>,
    );
    const backdrop = screen.getByTestId('bottom-drawer-backdrop');
    fireEvent.click(backdrop);
    expect(onClose).toHaveBeenCalled();
  });

  it('reflects open state in data-state attribute', () => {
    const { rerender } = render(
      <BottomDrawer open onClose={vi.fn()} title="Raw">
        <p>x</p>
      </BottomDrawer>,
    );
    expect(screen.getByTestId('bottom-drawer').dataset.state).toBe('open');
    rerender(
      <BottomDrawer open={false} onClose={vi.fn()} title="Raw">
        <p>x</p>
      </BottomDrawer>,
    );
    // When closed, the testid root may be unmounted — assert absence.
    expect(screen.queryByTestId('bottom-drawer')).toBeNull();
  });
});
