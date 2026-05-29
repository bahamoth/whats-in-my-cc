// webui/src/components/replay/timeline/viewport.ts
export interface Viewport { t0: number; t1: number; }

export function fit(extent: [number, number]): Viewport {
  return { t0: extent[0], t1: extent[1] };
}

/** factor < 1 zooms in (narrows), > 1 zooms out; focus time stays at the same fraction. */
export function zoomAt(v: Viewport, factor: number, focus: number): Viewport {
  const width = (v.t1 - v.t0) * factor;
  const frac = (focus - v.t0) / (v.t1 - v.t0);
  const t0 = focus - frac * width;
  return { t0, t1: t0 + width };
}

export function pan(v: Viewport, deltaT: number): Viewport {
  return { t0: v.t0 + deltaT, t1: v.t1 + deltaT };
}

export function clamp(v: Viewport, extent: [number, number]): Viewport {
  const extentWidth = extent[1] - extent[0];
  let width = Math.min(v.t1 - v.t0, extentWidth);
  let t0 = v.t0;
  let t1 = t0 + width;
  if (t0 < extent[0]) { t0 = extent[0]; t1 = t0 + width; }
  if (t1 > extent[1]) { t1 = extent[1]; t0 = t1 - width; }
  return { t0, t1 };
}
