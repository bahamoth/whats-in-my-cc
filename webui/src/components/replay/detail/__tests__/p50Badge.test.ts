import { describe, expect, it } from 'vitest';
import { p50Badge } from '../p50Badge';

// t()는 key+arg를 그대로 합성하는 스텁 — 카탈로그 실값은 parity 테스트가 잠근다.
const t = ((key: string, arg?: unknown) => `${key}:${JSON.stringify(arg)}`) as never;

describe('p50Badge (PR-3 §3d)', () => {
  it('값/중앙값 비율을 x.x×로 만든다', () => {
    const b = p50Badge(3000, { p50: 1000, n: 10 }, t);
    expect(b).toEqual({ text: 'metric.badge.median:"3.0"', lowSample: false });
  });
  it('n<3이면 표본 부족 배지', () => {
    const b = p50Badge(3000, { p50: 1000, n: 2 }, t);
    expect(b!.lowSample).toBe(true);
  });
  it('값 없음·stat 없음·p50 null이면 배지 없음', () => {
    expect(p50Badge(null, { p50: 1000, n: 10 }, t)).toBeNull();
    expect(p50Badge(3000, undefined, t)).toBeNull();
    expect(p50Badge(3000, { p50: null, n: 5 }, t)).toBeNull();
  });
});
