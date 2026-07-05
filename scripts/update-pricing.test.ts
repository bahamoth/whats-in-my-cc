/** node:test — 루트 스크립트 테스트 (webui vitest 밖). Node 22+.
 *  Run: node --test scripts/update-pricing.test.ts */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parsePricingPage, diffRates, type PricingJson } from './update-pricing.ts';

const fixture = readFileSync(
  new URL('../tests/fixtures/pricing/real/pricing-page-2026-07-05.html', import.meta.url),
  'utf8',
);
const pricing: PricingJson = JSON.parse(
  readFileSync(new URL('../pricing.json', import.meta.url), 'utf8'),
);

test('동결 페이지 파싱 결과가 체크인된 가격표와 정확히 일치한다', () => {
  const parsed = parsePricingPage(fixture, Object.keys(pricing.models));
  assert.deepEqual(diffRates(pricing, parsed), []);
});

test('등록 모델이 페이지에 없으면 throw (구조 변경 신호)', () => {
  assert.throws(() => parsePricingPage('<html><body>empty</body></html>', ['claude-fable-5']));
});

test('단가 변동은 diff 줄로 드러난다', () => {
  const parsed = parsePricingPage(fixture, Object.keys(pricing.models));
  parsed['claude-fable-5'] = { ...parsed['claude-fable-5'], output_per_mtok: 60 };
  const diff = diffRates(pricing, parsed);
  assert.equal(diff.length, 1);
  assert.match(diff[0], /claude-fable-5\.output_per_mtok: 50 -> 60/);
});
