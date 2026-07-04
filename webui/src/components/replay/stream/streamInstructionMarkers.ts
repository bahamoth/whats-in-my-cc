/** 지시문 변경 마커 삽입 (B-12) — buildStreamModel 결과에 대한 순수 후처리.
 *  같은 (source, path)의 연속 관측 쌍이 변경이고, 마커는 관측 시각보다 늦은
 *  첫 항목 앞에 놓인다(항목 시각 = primaryEventId의 observed_at). */
import type { InstructionObservationDto } from '../../../api/types';
import type { InstructionMarkerItem, StreamItem } from './streamModel';
import { primaryEventId } from './streamKeyboard';

export function insertInstructionMarkers(
  items: StreamItem[],
  obs: InstructionObservationDto[],
  timeOf: (eventId: string) => string | undefined,
): StreamItem[] {
  const byFile = new Map<string, InstructionObservationDto[]>();
  for (const o of obs) {
    const k = `${o.source}|${o.path}`;
    if (!byFile.has(k)) byFile.set(k, []);
    byFile.get(k)!.push(o);
  }
  const markers: InstructionMarkerItem[] = [];
  for (const [, g] of byFile) {
    const sorted = [...g].sort((a, b) => a.observed_at.localeCompare(b.observed_at));
    for (let i = 1; i < sorted.length; i++) {
      markers.push({
        type: 'instruction-marker',
        id: `im_${sorted[i].source}_${sorted[i].content_sha256.slice(0, 8)}_${i}`,
        observedAt: sorted[i].observed_at,
        source: sorted[i].source,
        beforeHash: sorted[i - 1].content_sha256,
        afterHash: sorted[i].content_sha256,
      });
    }
  }
  if (markers.length === 0) return items;
  markers.sort((a, b) => a.observedAt.localeCompare(b.observedAt));

  const out: StreamItem[] = [];
  let mi = 0;
  for (const item of items) {
    const eid = primaryEventId(item);
    const t = eid ? timeOf(eid) : undefined;
    while (mi < markers.length && t !== undefined && markers[mi].observedAt <= t) {
      out.push(markers[mi++]);
    }
    out.push(item);
  }
  while (mi < markers.length) out.push(markers[mi++]);
  return out;
}
