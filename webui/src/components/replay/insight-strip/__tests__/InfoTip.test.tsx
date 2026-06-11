import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { InfoTip } from '../InfoTip';

describe('InfoTip', () => {
  it('is closed by default — no tooltip in the DOM', () => {
    render(<InfoTip label="cache" text="cache-hit explanation" />);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('opens on hover and shows the explanatory text', () => {
    render(<InfoTip label="cache" text="cache-hit explanation" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip')).toHaveTextContent('cache-hit explanation');
  });

  it('closes again on mouse leave when not click-pinned', () => {
    render(<InfoTip label="cache" text="explain" />);
    const t = screen.getByTestId('infotip-trigger');
    fireEvent.mouseEnter(t);
    fireEvent.mouseLeave(t);
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('click pins it open; mouse leave then keeps it open; second click closes it', () => {
    render(<InfoTip label="cache" text="explain" />);
    const t = screen.getByTestId('infotip-trigger');
    fireEvent.click(t);
    expect(screen.getByRole('tooltip')).toBeInTheDocument();
    fireEvent.mouseLeave(t);
    expect(screen.getByRole('tooltip')).toBeInTheDocument(); // pinned
    fireEvent.click(t);
    expect(screen.queryByRole('tooltip')).toBeNull(); // unpinned + not hovered
  });

  it('the trigger does not bubble its click to an enclosing handler', () => {
    let outer = 0;
    render(
      <div onClick={() => { outer += 1; }}>
        <InfoTip label="cache" text="explain" />
      </div>,
    );
    fireEvent.click(screen.getByTestId('infotip-trigger'));
    expect(outer).toBe(0); // stopPropagation so opening the tip never expands the card
  });
});

describe('InfoTip placement — flips above when the bubble would be clipped below', () => {
  // Detail-panel rows near the bottom of the scrollable panel: a bubble that
  // always opens downward (top: 100%) is cut off by the panel overflow. When
  // there is not enough room below the trigger, the bubble must open upward.
  // jsdom has no layout, so the bubble rect is stubbed at the prototype level.
  afterEach(() => {
    vi.restoreAllMocks();
  });
  it('opens below by default when there is room', () => {
    window.innerHeight = 800;
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
      { top: 100, bottom: 160, left: 0, right: 200, width: 200, height: 60 } as DOMRect,
    );
    render(<InfoTip label="m" text="explain" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip').getAttribute('data-placement')).toBe('below');
  });
  it('flips above when the bubble bottom would pass the viewport bottom', () => {
    window.innerHeight = 800;
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
      { top: 796, bottom: 856, left: 0, right: 200, width: 200, height: 60 } as DOMRect,
    );
    render(<InfoTip label="m" text="explain" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip').getAttribute('data-placement')).toBe('above');
  });
  it('flips above when a scrollable ANCESTOR clips below — even with viewport room (detail panel case)', () => {
    // Live-captured 2026-06-11: bubble bottom 773 > detail-panel clip bottom 758
    // while window.innerHeight was 1,485 — a viewport-only check leaves the tip
    // clipped by the panel overflow. The clip boundary must be the nearest
    // overflowing ancestor, not the window.
    window.innerHeight = 2000;
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (this: Element) {
      if (this.getAttribute('role') === 'tooltip') {
        return { top: 722, bottom: 773, left: 0, right: 200, width: 200, height: 51 } as DOMRect;
      }
      if ((this as HTMLElement).dataset?.clip === '1') {
        return { top: 100, bottom: 758, left: 0, right: 400, width: 400, height: 658 } as DOMRect;
      }
      return { top: 700, bottom: 722, left: 0, right: 16, width: 16, height: 16 } as DOMRect;
    });
    render(
      <div data-clip="1" style={{ overflowY: 'auto' }}>
        <InfoTip label="m" text="explain" />
      </div>,
    );
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip').getAttribute('data-placement')).toBe('above');
  });
});

describe('InfoTip horizontal alignment — flips right when clipped at the right edge', () => {
  // Dogfooding 2026-06-11: the cost card (rightmost) and 요청출처 tooltips opened
  // left-anchored (left: 0) and ran off the viewport's right edge, unreadable.
  // The bubble must right-align when it would pass the right clip boundary.
  afterEach(() => {
    vi.restoreAllMocks();
  });
  it('left-aligned by default when there is room on the right', () => {
    window.innerWidth = 1000;
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
      { top: 100, bottom: 160, left: 100, right: 380, width: 280, height: 60 } as DOMRect,
    );
    render(<InfoTip label="m" text="explain" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip').getAttribute('data-align')).toBe('left');
  });
  it('flips right-aligned when the bubble right would pass the viewport right edge', () => {
    window.innerWidth = 1000;
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
      { top: 100, bottom: 160, left: 900, right: 1180, width: 280, height: 60 } as DOMRect,
    );
    render(<InfoTip label="m" text="explain" />);
    fireEvent.mouseEnter(screen.getByTestId('infotip-trigger'));
    expect(screen.getByRole('tooltip').getAttribute('data-align')).toBe('right');
  });
});
