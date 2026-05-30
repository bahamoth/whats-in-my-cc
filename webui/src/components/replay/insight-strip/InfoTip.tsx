/**
 * slice-7 — `?` tooltip for a card's long-form provenance / explanation
 * (design spec §2 P3, §5). Opens on hover OR click; click pins it open so the
 * user can read it without keeping the pointer on the trigger; a second click
 * closes it. The trigger stops click propagation so it never toggles the
 * enclosing card's expand state.
 */
import { useState } from 'react';
import styles from './InfoTip.module.css';

interface InfoTipProps {
  /** Short subject (used for the aria-label, e.g. the card title). */
  label: string;
  /** Long-form explanation shown in the tooltip body. */
  text: string;
}

export function InfoTip({ label, text }: InfoTipProps) {
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const open = hovered || pinned;

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
        <span role="tooltip" className={styles.bubble}>
          {text}
        </span>
      )}
    </span>
  );
}
