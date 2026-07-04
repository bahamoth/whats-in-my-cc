/** B-12 — 마커 파생(연속 관측 쌍)과 시간 위치 삽입을 잠근다. */
import { describe, expect, it } from 'vitest';
import { insertInstructionMarkers } from '../streamInstructionMarkers';
import type { StreamItem } from '../streamModel';
import type { InstructionObservationDto } from '../../../../api/types';

const msg = (id: string): StreamItem =>
  ({ type: 'message', eventId: id, role: 'user' }) as unknown as StreamItem;
const ob = (sha: string, at: string): InstructionObservationDto => ({
  source: 'project',
  path: '/w/CLAUDE.md',
  content_sha256: sha,
  observed_at: at,
});

const times: Record<string, string> = {
  e1: '2026-07-04T10:00:00+00:00',
  e2: '2026-07-04T12:00:00+00:00',
};
const timeOf = (id: string) => times[id];

describe('insertInstructionMarkers', () => {
  it('연속 관측 쌍이 변경 마커가 되어 시간 위치에 삽입된다', () => {
    const items = [msg('e1'), msg('e2')];
    const out = insertInstructionMarkers(
      items,
      [ob('aaaa', '2026-07-04T09:00:00+00:00'), ob('bbbb', '2026-07-04T11:00:00+00:00')],
      timeOf,
    );
    expect(out.map((i) => i.type)).toEqual(['message', 'instruction-marker', 'message']);
    const m = out[1] as Extract<StreamItem, { type: 'instruction-marker' }>;
    expect(m.beforeHash).toBe('aaaa');
    expect(m.afterHash).toBe('bbbb');
  });
  it('단일 관측(변경 없음)은 마커를 만들지 않는다', () => {
    const items = [msg('e1')];
    const out = insertInstructionMarkers(items, [ob('aaaa', '2026-07-04T09:00:00+00:00')], timeOf);
    expect(out).toEqual(items);
  });
  it('모든 항목보다 늦은 변경은 끝에 붙는다', () => {
    const out = insertInstructionMarkers(
      [msg('e1')],
      [ob('aaaa', '2026-07-04T09:00:00+00:00'), ob('bbbb', '2026-07-04T13:00:00+00:00')],
      timeOf,
    );
    expect(out.map((i) => i.type)).toEqual(['message', 'instruction-marker']);
  });
});
