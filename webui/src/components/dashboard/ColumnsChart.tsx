// 세션 축 컬럼 차트 — outcome(3-스택)과 metric strip(1-스택)이 공유하는
// 유일한 차트 프리미티브. 데이터-끝 4px 라운드·세그먼트 2px 갭·hover 툴팁·
// 컬럼 클릭 → 세션 replay 딥링크. 값 텍스트는 텍스트 토큰만 쓴다(series
// 색은 마크에만 — dataviz 규칙).
import { ReactNode, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import type { SessionSeriesRowDto } from '../../api/types';
import { gridColumns } from './columns';
import styles from './ColumnsChart.module.css';

export type ColumnStack = {
  key: string;
  /** CSS color — 마크 전용. */
  color: string;
  /** rows와 평행한 값 배열. */
  values: number[];
};

interface ColumnsChartProps {
  rows: SessionSeriesRowDto[];
  /** 아래→위 순서의 스택. */
  stacks: ColumnStack[];
  /** 플롯 높이(px). */
  height: number;
  ariaLabel: string;
  /** hover 툴팁 내용 — null이면 툴팁 없음. */
  tooltip: (index: number) => ReactNode;
  openSessionLabel: (id: string) => string;
}

type Tip = { index: number; x: number; y: number };

export function ColumnsChart({
  rows,
  stacks,
  height,
  ariaLabel,
  tooltip,
  openSessionLabel,
}: ColumnsChartProps) {
  const navigate = useNavigate();
  const [tip, setTip] = useState<Tip | null>(null);
  const n = rows.length;
  const totals = rows.map((_, i) => stacks.reduce((acc, s) => acc + (s.values[i] ?? 0), 0));
  const max = Math.max(1, ...totals);

  return (
    <div className={styles.wrap}>
      <div
        className={styles.plot}
        style={{ height, gridTemplateColumns: gridColumns(n) }}
        role="img"
        aria-label={ariaLabel}
      >
        {rows.map((row, i) => (
          <button
            key={row.session_id}
            type="button"
            className={styles.col}
            aria-label={openSessionLabel(row.session_id)}
            onClick={() => navigate(`/sessions/${encodeURIComponent(row.session_id)}`)}
            onMouseEnter={(e) => setTip({ index: i, x: e.clientX, y: e.clientY })}
            onMouseMove={(e) => setTip({ index: i, x: e.clientX, y: e.clientY })}
            onMouseLeave={() => setTip(null)}
            onFocus={() => setTip(null)}
          >
            {stacks.map((s) => {
              const v = s.values[i] ?? 0;
              if (v <= 0) return null;
              return (
                <span
                  key={s.key}
                  className={styles.seg}
                  style={{ height: `${(v / max) * 100}%`, background: s.color }}
                />
              );
            })}
          </button>
        ))}
      </div>
      {tip !== null && tooltip(tip.index) !== null && (
        <div className={styles.tip} style={{ left: tip.x + 12, top: tip.y + 14 }} role="status">
          {tooltip(tip.index)}
        </div>
      )}
    </div>
  );
}
