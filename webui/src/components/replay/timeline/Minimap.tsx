// webui/src/components/replay/timeline/Minimap.tsx
/**
 * R4 — Minimap brush: a full-extent overview with a draggable window rect.
 * Plan: docs/superpowers/plans/2026-05-29-witmcc-redesign-v2-R4-timeline.md Task 5.
 *
 * Geometry is computed entirely from the `width` prop and the `extent`/`viewport`
 * values — never from getBoundingClientRect — so tests are deterministic in jsdom.
 *
 * In the browser we also use `trackRef.current.getBoundingClientRect().left` to
 * translate raw clientX into a track-relative px offset, but we fall back to 0
 * so tests (where getBoundingClientRect always returns zeros) still work.
 */
import { useCallback, useRef } from 'react';
import { clamp, type Viewport } from './viewport';
import styles from './Minimap.module.css';

export interface MinimapProps {
  extent: [number, number];
  viewport: Viewport;
  onChange: (v: Viewport) => void;
  width?: number;
}

const TRACK_HEIGHT = 18;

export function Minimap({ extent, viewport, onChange, width = 600 }: MinimapProps) {
  const [extentStart, extentEnd] = extent;
  const extentDuration = extentEnd - extentStart;
  const trackRef = useRef<SVGSVGElement>(null);

  // Linear mapping: time → pixel within the track
  const toPx = useCallback(
    (t: number) => ((t - extentStart) / extentDuration) * width,
    [extentStart, extentDuration, width]
  );

  // Pixel → time
  const toTime = useCallback(
    (px: number) => extentStart + (px / width) * extentDuration,
    [extentStart, extentDuration, width]
  );

  // Window geometry
  const winX = toPx(viewport.t0);
  const winW = Math.max(2, toPx(viewport.t1) - winX);
  const viewportDuration = viewport.t1 - viewport.t0;

  // Get track-left offset (0 in jsdom; real value in browser)
  const trackLeft = useCallback((): number => {
    if (!trackRef.current) return 0;
    if (typeof trackRef.current.getBoundingClientRect !== 'function') return 0;
    return trackRef.current.getBoundingClientRect().left;
  }, []);

  // Recenter viewport on click/drag: center the current window on the clicked time
  const centerOn = useCallback(
    (clientX: number) => {
      const px = clientX - trackLeft();
      const focusT = toTime(px);
      const half = viewportDuration / 2;
      const raw: Viewport = { t0: focusT - half, t1: focusT + half };
      onChange(clamp(raw, extent));
    },
    [toTime, viewportDuration, extent, onChange, trackLeft]
  );

  const draggingRef = useRef(false);

  const onMouseDown = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      draggingRef.current = true;
      centerOn(e.clientX);
    },
    [centerOn]
  );

  const onMouseMove = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      if (!draggingRef.current) return;
      centerOn(e.clientX);
    },
    [centerOn]
  );

  const onMouseUp = useCallback(() => {
    draggingRef.current = false;
  }, []);

  return (
    <div className={styles.root} style={{ width }}>
      <svg
        ref={trackRef}
        data-testid="minimap-track"
        className={styles.track}
        width={width}
        height={TRACK_HEIGHT}
        viewBox={`0 0 ${width} ${TRACK_HEIGHT}`}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onMouseLeave={onMouseUp}
      >
        {/* Track background */}
        <rect
          className={styles.trackBg}
          x={0}
          y={0}
          width={width}
          height={TRACK_HEIGHT}
        />

        {/* Brush window */}
        <rect
          data-testid="brush-window"
          className={styles.window}
          x={winX}
          y={1}
          width={winW}
          height={TRACK_HEIGHT - 2}
        />
      </svg>
    </div>
  );
}
