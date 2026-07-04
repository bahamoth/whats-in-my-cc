/**
 * ECharts tree-shaken 래퍼 — 대시보드가 쓰는 차트/컴포넌트만 등록한다.
 * option은 통째 교체(notMerge)로 다룬다: 파생은 전부 순수 빌더 함수가 만들고
 * 이 컴포넌트는 lifecycle(초기화/리사이즈/폐기)만 책임진다.
 */
import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import type { EChartsCoreOption } from 'echarts/core';
import { BarChart, LineChart, ScatterChart, SankeyChart } from 'echarts/charts';
import {
  GridComponent,
  TooltipComponent,
  DataZoomComponent,
  DataZoomInsideComponent,
  DataZoomSliderComponent,
  MarkLineComponent,
  LegendComponent,
} from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';

echarts.use([
  BarChart,
  LineChart,
  ScatterChart,
  SankeyChart,
  GridComponent,
  TooltipComponent,
  DataZoomComponent,
  DataZoomInsideComponent,
  DataZoomSliderComponent,
  MarkLineComponent,
  LegendComponent,
  CanvasRenderer,
]);

export function EChart({
  option,
  height,
  group,
  onEvents,
}: {
  option: EChartsCoreOption;
  height: number;
  group?: string;
  onEvents?: Record<string, (params: unknown) => void>;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const chart = useRef<ReturnType<typeof echarts.init>>();
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const c = echarts.init(el);
    chart.current = c;
    if (group) {
      c.group = group;
      echarts.connect(group);
    }
    for (const [ev, fn] of Object.entries(onEvents ?? {})) c.on(ev, fn);
    const ro = new ResizeObserver(() => c.resize());
    ro.observe(el);
    return () => {
      ro.disconnect();
      c.dispose();
    };
    // group/onEvents는 마운트 시 1회 바인딩 — 호출부가 정적으로 넘긴다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    chart.current?.setOption(option, { notMerge: true });
  }, [option]);
  return <div ref={ref} style={{ height, width: '100%' }} />;
}
