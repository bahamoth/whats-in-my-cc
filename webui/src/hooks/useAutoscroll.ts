// Explicit autoscroll (stick-to-bottom) controller for the conversation
// stream. It is the SINGLE owner of the stream's scroll-position policy,
// replacing the implicit react-virtual `followOnAppend` + the hand-rolled 2s
// `followInitRef` bottom-pin (which competed and produced the live-append
// jitter / "jumps to the tip from the top" behaviour).
//
// Contract:
//   - starts following; lands at the tip on first paint.
//   - user scrolls up → stops following (autoscroll off).
//   - user returns to the bottom → resumes following, clears the new-count.
//   - a tip append while following → scroll to the bottom (instant).
//   - a tip append while NOT following → bump `newCount` (the "N new" badge);
//     the viewport is left alone.
//   - a prepend (older history) is ignored here — react-virtual's
//     `anchorTo:'end'` keeps the viewport stable across it.
//   - enable()/disable() are the pill's click actions.
//
// The hook does NOT attach its own scroll listener; the consumer wires the
// returned `onScroll` to the scroll element. This keeps it DOM-light and
// unit-testable with a plain object ref (jsdom has no layout); real-layout
// behaviour is verified by browser smoke.

import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import {
  isAtBottom,
  classifyChange,
  DEFAULT_BOTTOM_THRESHOLD,
  type ItemsSignature,
} from '../components/replay/stream/autoscrollPolicy';

export interface UseAutoscrollResult {
  /** Whether the stream is following the live tip. */
  autoscroll: boolean;
  /** Tip appends accumulated while NOT following (the "N new" badge). */
  newCount: number;
  /** Pill click while OFF: jump to the bottom and resume following. */
  enable: () => void;
  /** Pill click while ON: stop following, stay put. */
  disable: () => void;
  /** Wire to the scroll element's `scroll` event. */
  onScroll: () => void;
  /** Scroll to the bottom AND mark it programmatic (so the resulting scroll
   *  event is not mistaken for a user gesture). The consumer calls this to keep
   *  the viewport pinned to the measured bottom while following as virtualized
   *  rows lazily measure and grow the content. No-op'd by callers when OFF. */
  pinToBottom: () => void;
}

export interface UseAutoscrollOpts {
  bottomThreshold?: number;
  /** Start detached (not following the live tip). Set when the page mounts on a
   *  `?selected=` deep-link, so live SSE backfill does not pull the window off
   *  the loadAround target before it can be scrolled into view. Default true. */
  initialFollow?: boolean;
  /** Whether reaching the BOTTOM of the loaded buffer may RESUME following the
   *  live tip. False when the window is a detached slice with newer events still
   *  to page (NOT the live tip): there, hitting the buffer bottom must
   *  forward-page (grow the window), not jump to the tail. Follow resumes only
   *  once the real tip is reached (this flips true) or via the explicit toggle
   *  (`enable()`, which is NOT gated by this). Default true. */
  canResumeFollow?: boolean;
}

// How far the reader must scroll UP from our last pinned position to count as a
// genuine "detach" gesture. We distinguish a user scroll-up (scrollTop moves
// DOWN, away from the bottom) from mere content growth while following
// (scrollHeight grows, scrollTop unchanged) by DIRECTION — far more robust than
// a time window, which would swallow a scroll-up that itself triggers a
// re-measure + pin.
const DETACH_PX = 40;

function scrollToBottom(el: HTMLElement): number {
  // instant — a smooth/animated jump fights live re-measurement and reads as jank.
  el.scrollTop = el.scrollHeight;
  return el.scrollTop;
}

export function useAutoscroll(
  scrollRef: RefObject<HTMLElement | null>,
  signature: ItemsSignature,
  opts: UseAutoscrollOpts = {},
): UseAutoscrollResult {
  const threshold = opts.bottomThreshold ?? DEFAULT_BOTTOM_THRESHOLD;

  const [autoscroll, setAutoscroll] = useState(opts.initialFollow ?? true);
  const [newCount, setNewCount] = useState(0);

  // Mirror autoscroll into a ref so the items-change effect reads the current
  // value without taking it as a dependency (which would re-run the effect on
  // every toggle, not just on item changes). Initialised from initialFollow so
  // a detached deep-link mount has ref AND state both false (a hardcoded `true`
  // here would make onScroll take the "following" branch on the first gesture).
  const autoscrollRef = useRef(opts.initialFollow ?? true);
  const setFollow = useCallback((v: boolean) => {
    autoscrollRef.current = v;
    setAutoscroll(v);
  }, []);

  // Whether hitting the buffer bottom may resume following (see opts doc).
  // Mirrored to a ref so onScroll reads the live value without re-subscribing.
  const canResumeFollowRef = useRef(opts.canResumeFollow ?? true);
  canResumeFollowRef.current = opts.canResumeFollow ?? true;

  const prevSigRef = useRef<ItemsSignature | null>(null);
  // The scrollTop of our last programmatic pin. onScroll compares against it to
  // tell a user scroll-up (top drops below this) from our own pin / content
  // growth (top stays at/above this).
  const lastPinnedTopRef = useRef<number>(0);

  // Scroll to the bottom and remember where we landed, so the scroll event this
  // produces (and any from content re-measuring) is not mistaken for a gesture.
  const pinToBottom = useCallback(() => {
    // Following-gated on the REF (not state): the consumer's measurement-settle
    // pin reads the autoscroll STATE, which can lag a synchronous detach by one
    // render. Refusing to pin while the fresh ref is false prevents yanking a
    // just-scrolled-up reader back to the tip. enable() flips the ref true first.
    if (!autoscrollRef.current) return;
    const el = scrollRef.current;
    if (!el) return;
    lastPinnedTopRef.current = scrollToBottom(el);
  }, [scrollRef]);

  // React to item-list changes: follow on tip-append, count while detached.
  useLayoutEffect(() => {
    const prev = prevSigRef.current;
    prevSigRef.current = signature;
    const el = scrollRef.current;
    if (!el) return;

    if (prev === null) {
      // first paint — land at the tip and remember the position so a later
      // user scroll-up is recognised (the consumer's measurement-settle pin
      // keeps us at the measured bottom as rows lazily grow).
      if (signature.count > 0) lastPinnedTopRef.current = scrollToBottom(el);
      return;
    }

    const { appended } = classifyChange(prev, signature);
    if (appended <= 0) return; // prepend / no tip growth → leave scroll alone

    if (autoscrollRef.current) {
      pinToBottom();
    } else {
      setNewCount((c) => c + appended);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature.first, signature.last, signature.count]);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const top = el.scrollTop;
    const atBottom = isAtBottom(
      { scrollTop: top, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight },
      threshold,
    );
    if (autoscrollRef.current) {
      // Following: detach ONLY on a genuine upward gesture — scrollTop dropped
      // below our last pinned position. Pure content growth (scrollHeight up,
      // scrollTop unchanged) and our own pin echo leave top >= pinned → stay ON.
      if (top < lastPinnedTopRef.current - DETACH_PX) {
        setFollow(false);
      }
    } else if (atBottom && canResumeFollowRef.current) {
      // Detached but the reader scrolled back to the bottom AND the window is at
      // the live tip → resume following. When newer pages still remain
      // (canResumeFollow false) the buffer bottom is NOT the tip — the consumer
      // forward-pages there instead, so we must not jump to the tail here.
      lastPinnedTopRef.current = top;
      setFollow(true);
      setNewCount(0);
    }
  }, [scrollRef, threshold, setFollow]);

  const enable = useCallback(() => {
    setFollow(true); // flip the ref true BEFORE pinning — pinToBottom is ref-gated
    pinToBottom();
    setNewCount(0);
  }, [pinToBottom, setFollow]);

  const disable = useCallback(() => {
    setFollow(false);
  }, [setFollow]);

  return { autoscroll, newCount, enable, disable, onScroll, pinToBottom };
}
