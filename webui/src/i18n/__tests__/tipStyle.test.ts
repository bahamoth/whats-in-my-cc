/**
 * 툴팁 카피 규칙 게이트 (CLAUDE.md '툴팁 카피 규칙'의 강제 장치).
 * `.tip`로 끝나는 모든 i18n 키에 적용:
 *  1) 강조 마크업 최소 1개 — **굵게**, [green|red|amber|violet|blue]색[/…], `코드`
 *  2) 120자 초과 툴팁은 줄바꿈(\n)으로 항목을 나눈다
 * 문법 SSOT: components/replay/insight-strip/InfoTip.renderTipMarkup.
 */
import { describe, expect, it } from 'vitest';
import { en } from '../catalog/en';
import { ko } from '../catalog/ko';

const EMPH = /\*\*|\[(?:green|red|amber|violet|blue)\]|`/;

for (const [name, cat] of [
  ['en', en],
  ['ko', ko],
] as const) {
  describe(`tooltip copy rules — ${name}`, () => {
    const tips = Object.entries(cat).filter(
      ([k, v]) => k.endsWith('.tip') && typeof v === 'string',
    ) as Array<[string, string]>;

    it('모든 툴팁이 존재하고 문자열이다', () => {
      expect(tips.length).toBeGreaterThan(0);
    });

    it('강조 마크업(굵게/색/코드) 최소 1개', () => {
      const bad = tips.filter(([, v]) => !EMPH.test(v)).map(([k]) => k);
      expect(bad).toEqual([]);
    });

    it('120자 초과 툴팁은 줄바꿈으로 항목 구분', () => {
      const bad = tips.filter(([, v]) => v.length > 120 && !v.includes('\n')).map(([k]) => k);
      expect(bad).toEqual([]);
    });
  });
}
