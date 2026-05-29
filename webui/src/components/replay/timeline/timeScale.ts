// webui/src/components/replay/timeline/timeScale.ts
import { scaleTime } from 'd3-scale';

export type TimeScale = ReturnType<typeof scaleTime<number, number>>;

export function makeTimeScale(domain: [number, number], range: [number, number]): TimeScale {
  return scaleTime().domain([new Date(domain[0]), new Date(domain[1])]).range(range);
}

export interface AxisTick { t: number; x: number; label: string; }

export function axisTicks(scale: TimeScale, width: number): AxisTick[] {
  // ~1 tick per 90px; d3 chooses nice time boundaries and the label format.
  const count = Math.max(2, Math.floor(width / 90));
  const fmt = scale.tickFormat(count);
  return scale.ticks(count).map((d) => ({ t: d.getTime(), x: scale(d), label: fmt(d) }));
}
