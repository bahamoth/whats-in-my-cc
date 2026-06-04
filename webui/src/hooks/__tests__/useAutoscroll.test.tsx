import { describe, expect, it } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useRef } from 'react';
import { useAutoscroll } from '../useAutoscroll';
import type { ItemsSignature } from '../../components/replay/stream/autoscrollPolicy';

// A plain stand-in for the scroll element. useAutoscroll only reads/writes
// scrollTop/scrollHeight/clientHeight and never attaches its own listeners (the
// consumer wires the returned onScroll), so a plain object suffices — jsdom has
// no real layout. Real-layout behaviour is covered by browser smoke.
function makeEl(scrollHeight: number, clientHeight: number, scrollTop = 0) {
  return { scrollHeight, clientHeight, scrollTop } as unknown as HTMLElement & {
    scrollHeight: number;
    clientHeight: number;
    scrollTop: number;
  };
}

function setup(initialSig: ItemsSignature, el: ReturnType<typeof makeEl>) {
  return renderHook(
    ({ sig }: { sig: ItemsSignature }) => {
      const ref = useRef(el);
      return useAutoscroll(ref, sig);
    },
    { initialProps: { sig: initialSig } },
  );
}

const sig = (first: string | null, last: string | null, count: number): ItemsSignature => ({
  first,
  last,
  count,
});

describe('useAutoscroll', () => {
  it('starts following and scrolls to the bottom on first paint', () => {
    const el = makeEl(1000, 100, 0);
    const { result } = setup(sig('a', 'c', 3), el);
    expect(result.current.autoscroll).toBe(true);
    expect(el.scrollTop).toBe(1000); // scrolled to bottom (instant)
    expect(result.current.newCount).toBe(0);
  });

  it('turns OFF when the user scrolls up away from the bottom', () => {
    const el = makeEl(1000, 100, 1000);
    const { result } = setup(sig('a', 'c', 3), el);
    act(() => {
      el.scrollTop = 200; // user scrolled up; dist = 700 > threshold
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(false);
  });

  it('turns back ON and clears the new-count when the user returns to the bottom', () => {
    const el = makeEl(1000, 100, 1000);
    const { result } = setup(sig('a', 'c', 3), el);
    act(() => {
      el.scrollTop = 200;
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(false);
    act(() => {
      el.scrollTop = 900; // dist = 0
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(true);
    expect(result.current.newCount).toBe(0);
  });

  it('while OFF, a tip append increments newCount and does NOT scroll', () => {
    const el = makeEl(1000, 100, 1000);
    const { result, rerender } = setup(sig('a', 'c', 3), el);
    act(() => {
      el.scrollTop = 200;
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(false);
    const topBefore = el.scrollTop;
    act(() => {
      el.scrollHeight = 1200; // taller content
      rerender({ sig: sig('a', 'e', 5) }); // 2 appended at tip
    });
    expect(result.current.newCount).toBe(2);
    expect(el.scrollTop).toBe(topBefore); // not yanked to bottom
  });

  it('while ON, a tip append scrolls to the bottom and keeps newCount at 0', () => {
    const el = makeEl(1000, 100, 1000);
    const { result, rerender } = setup(sig('a', 'c', 3), el);
    expect(result.current.autoscroll).toBe(true);
    act(() => {
      el.scrollHeight = 1200;
      rerender({ sig: sig('a', 'e', 5) });
    });
    expect(el.scrollTop).toBe(1200); // followed to the new bottom
    expect(result.current.newCount).toBe(0);
  });

  it('ignores the programmatic scroll event that follows an ON-append (no flip)', () => {
    const el = makeEl(1000, 100, 1000);
    const { result, rerender } = setup(sig('a', 'c', 3), el);
    act(() => {
      el.scrollHeight = 1200;
      rerender({ sig: sig('a', 'e', 5) }); // follow → scrollTop=1200 set programmatically
    });
    // the resulting scroll event must not be mistaken for a user gesture
    act(() => {
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(true);
  });

  it('while ON, content growth (scrollHeight up, scrollTop same) keeps following', () => {
    // Regression: lazy row measurement grows scrollHeight while scrollTop holds
    // — that must NOT be read as a user scroll-up and detach. Only a scrollTop
    // DROP below the pinned position detaches.
    const el = makeEl(1000, 100, 1000);
    const { result } = setup(sig('a', 'c', 3), el);
    expect(result.current.autoscroll).toBe(true);
    act(() => {
      el.scrollHeight = 1500; // content measured taller; scrollTop unchanged (1000)
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(true);
  });

  it('a prepend (older history) does not change following or newCount', () => {
    const el = makeEl(1000, 100, 1000);
    const { result, rerender } = setup(sig('c', 'e', 3), el);
    expect(result.current.autoscroll).toBe(true);
    act(() => {
      el.scrollHeight = 3000; // older content added above
      rerender({ sig: sig('a', 'e', 6) }); // first changed, last same
    });
    expect(result.current.autoscroll).toBe(true);
    expect(result.current.newCount).toBe(0);
  });

  it('enable() jumps to the bottom, follows, and clears newCount', () => {
    const el = makeEl(1000, 100, 1000);
    const { result } = setup(sig('a', 'c', 3), el);
    act(() => {
      el.scrollTop = 100;
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(false);
    act(() => {
      result.current.enable();
    });
    expect(result.current.autoscroll).toBe(true);
    expect(el.scrollTop).toBe(1000);
    expect(result.current.newCount).toBe(0);
  });

  it('pinToBottom() is a no-op while detached (guards against stale-state pin)', () => {
    // The consumer's measurement-settle effect gates on the autoscroll STATE,
    // which can lag a synchronous detach by one render. pinToBottom must itself
    // refuse to pin when the (fresh) follow ref is false, or it would yank a
    // just-scrolled-up reader back to the tip — the exact jitter this PR kills.
    const el = makeEl(1000, 100, 1000);
    const { result } = setup(sig('a', 'c', 3), el);
    act(() => {
      el.scrollTop = 200; // user scrolls up → detaches (ref false synchronously)
      result.current.onScroll();
    });
    expect(result.current.autoscroll).toBe(false);
    act(() => {
      result.current.pinToBottom(); // measurement-settle pin while detached
    });
    expect(el.scrollTop).toBe(200); // viewport held, NOT yanked to 1000
  });

  it('disable() stops following without moving the viewport', () => {
    const el = makeEl(1000, 100, 1000);
    const { result } = setup(sig('a', 'c', 3), el);
    const top = el.scrollTop;
    act(() => {
      result.current.disable();
    });
    expect(result.current.autoscroll).toBe(false);
    expect(el.scrollTop).toBe(top);
  });
});
