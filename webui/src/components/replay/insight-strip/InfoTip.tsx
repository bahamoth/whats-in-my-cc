/**
 * slice-7 — `?` tooltip for a card's long-form provenance / explanation
 * (design spec §2 P3, §5). Opens on hover OR click; click pins it open so the
 * user can read it without keeping the pointer on the trigger; a second click
 * closes it. The trigger stops click propagation so it never toggles the
 * enclosing card's expand state.
 *
 * Placement: opens below the trigger by default; when the bubble would be
 * clipped at the bottom (detail-panel rows near the panel edge — the bubble
 * is absolutely positioned inside a scrollable ancestor, so overflowing its
 * bottom cuts it off), it flips ABOVE the trigger. Measured after render so
 * the decision uses the bubble's real height.
 */
import { useLayoutEffect, useRef, useState } from 'react';
import styles from './InfoTip.module.css';

type Placement = 'below' | 'above';

/** The y where the bubble visually gets cut off: the nearest ancestor that
 *  clips its overflow (the detail panel is `overflow-y: auto` — an absolutely
 *  positioned bubble inside it is clipped at the PANEL bottom, which can sit
 *  well above the viewport bottom), falling back to the viewport. */
function clipBottomFor(el: HTMLElement): number {
  let n = el.parentElement;
  while (n && n !== document.body) {
    const overflowY = getComputedStyle(n).overflowY;
    if (overflowY === 'auto' || overflowY === 'scroll' || overflowY === 'hidden') {
      return Math.min(n.getBoundingClientRect().bottom, window.innerHeight);
    }
    n = n.parentElement;
  }
  return window.innerHeight;
}

interface InfoTipProps {
  /** Short subject (used for the aria-label, e.g. the card title). */
  label: string;
  /** Long-form explanation shown in the tooltip body. */
  text: string;
}

export function InfoTip({ label, text }: InfoTipProps) {
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [placement, setPlacement] = useState<Placement>('below');
  const bubbleRef = useRef<HTMLSpanElement | null>(null);
  const open = hovered || pinned;

  // Decide below/above from the rendered bubble's rect: flip when its bottom
  // would pass the viewport bottom. Runs on every open (rects change as the
  // panel scrolls); resets to 'below' between opens so a tip that fit once
  // does not stay flipped after the layout changed.
  useLayoutEffect(() => {
    if (!open) {
      setPlacement('below');
      return;
    }
    const bubble = bubbleRef.current;
    if (!bubble) return;
    const rect = bubble.getBoundingClientRect();
    if (rect.height > 0 && rect.bottom > clipBottomFor(bubble)) {
      setPlacement('above');
    }
  }, [open, text]);

  return (
    <span className={styles.wrap}>
      <button
        type="button"
        data-testid="infotip-trigger"
        className={styles.trigger}
        aria-label={`${label} 설명`}
        aria-expanded={open}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onClick={(e) => {
          e.stopPropagation();
          setPinned((p) => !p);
        }}
      >
        ?
      </button>
      {open && (
        <span
          role="tooltip"
          ref={bubbleRef}
          data-placement={placement}
          className={placement === 'above' ? `${styles.bubble} ${styles.bubbleAbove}` : styles.bubble}
        >
          {text}
        </span>
      )}
    </span>
  );
}
