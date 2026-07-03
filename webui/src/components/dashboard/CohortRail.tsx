// 코호트 레일 — 이 페이지의 시그니처. 세션 축 위에 환경(모델 집합·CC 버전)
// 코호트를 세그먼트 밴드로 얹는다. 경계 룰 자체는 페이지 오버레이가 그린다
// (모든 블록 관통 정렬) — 여기는 밴드와 직접 라벨만.
//
// 색 규칙(dataviz): 모델 밴드는 categorical — distinct 라벨을 세션 수
// 빈도순으로 고정 슬롯 4개에 배정한다(같은 데이터 = 같은 색; 윈도우가
// 바뀌어도 빈도 순위가 같으면 안정). 슬롯 초과분은 categorical hue가 아니라
// 중립 슬레이트로 접는다("Other" 접기 — 서로 다른 꼬리 코호트가 한 hue로
// 뭉개져 보이지 않게). 정체성은 항상 직접 라벨 + title이 전달한다.
// CC 버전 밴드는 순서 척도라 한 hue 두 shade 교대 + 직접 라벨.
import type { CohortSegment } from '../../lib/seriesView';
import { GAP_PX } from './columns';
import styles from './CohortRail.module.css';

/* dataviz validator 통과 세트(dark surface #11141b): blue/yellow/violet/orange. */
const MODEL_SLOTS = ['#3987e5', '#c98500', '#9085e9', '#d95926'];
/* 슬롯 초과분("Other" 접기) — 중립 슬레이트 de-emphasis. */
const MODEL_OVERFLOW = '#48536b';
/* CC 버전 밴드 — accent 계열 두 shade 교대(정체성은 직접 라벨이 전달). */
const CC_SHADES = ['#24407c', '#3a5fb8'];

interface CohortRailProps {
  bandLabel: string;
  segments: CohortSegment[];
  /** 세션 수 — 축 분모. */
  total: number;
  /** 'categorical'(모델) 또는 'ordinal'(CC 버전). */
  kind: 'categorical' | 'ordinal';
  unknownLabel: string;
}

function segStyle(seg: CohortSegment, total: number): React.CSSProperties {
  const span = seg.end - seg.start + 1;
  return {
    left: `calc((100% - ${(total - 1) * GAP_PX}px) * ${seg.start} / ${total} + ${seg.start * GAP_PX}px)`,
    width: `calc((100% - ${(total - 1) * GAP_PX}px) * ${span} / ${total} + ${(span - 1) * GAP_PX}px)`,
  };
}

/** 표시 라벨 — 'claude-' 접두는 밴드에서 정보가 없어 떼어낸다(전체 이름은
 *  title 툴팁이 유지). */
function displayLabel(label: string): string {
  return label.replaceAll('claude-', '');
}

/** 빈도순(동률은 사전순) 상위 4 라벨 → 슬롯, 나머지 → overflow 슬레이트. */
function slotMap(segments: CohortSegment[]): Map<string, string> {
  const weight = new Map<string, number>();
  for (const s of segments) {
    if (!s.known) continue;
    weight.set(s.label, (weight.get(s.label) ?? 0) + (s.end - s.start + 1));
  }
  const ranked = [...weight.entries()].sort((a, b) =>
    b[1] !== a[1] ? b[1] - a[1] : a[0].localeCompare(b[0]),
  );
  const map = new Map<string, string>();
  ranked.forEach(([label], i) => {
    map.set(label, i < MODEL_SLOTS.length ? MODEL_SLOTS[i] : MODEL_OVERFLOW);
  });
  return map;
}

export function CohortRail({ bandLabel, segments, total, kind, unknownLabel }: CohortRailProps) {
  const slots = kind === 'categorical' ? slotMap(segments) : null;
  const colorOf = (seg: CohortSegment, index: number): string =>
    slots ? (slots.get(seg.label) ?? MODEL_OVERFLOW) : CC_SHADES[index % 2];
  return (
    <div className={styles.band}>
      <span className={styles.bandLabel}>{bandLabel}</span>
      <div className={styles.rail}>
        {segments.map((seg, i) =>
          seg.known ? (
            <span
              key={`${seg.start}-${seg.label}`}
              className={styles.seg}
              style={{ ...segStyle(seg, total), background: colorOf(seg, i) }}
              title={seg.label}
            >
              {displayLabel(seg.label)}
            </span>
          ) : (
            <span
              key={`${seg.start}-unknown`}
              className={styles.segUnknown}
              style={segStyle(seg, total)}
              title={unknownLabel}
            >
              {unknownLabel}
            </span>
          ),
        )}
      </div>
    </div>
  );
}
