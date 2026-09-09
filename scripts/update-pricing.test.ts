/** node:test — 루트 스크립트 테스트 (webui vitest 밖). Node 22+.
 *  Run: node --test scripts/update-pricing.test.ts */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  parsePricingPage,
  htmlToText,
  diffRates,
  serializePricing,
  formatRate,
  detectNewModels,
  type PricingJson,
} from './update-pricing.ts';

const SCRIPT = fileURLToPath(new URL('./update-pricing.ts', import.meta.url));
// 현행 동결 fixture(2026-09-09): 표에 "Claude Fable 5.1"·"Claude Mythos 5.1"·"Claude Opus 5"
// 행이 "Claude Fable 5"·"Claude Mythos 5" 행보다 앞에 있다 — 표시명 접두 충돌의 실물.
const FIXTURE_URL = new URL('../tests/fixtures/pricing/real/pricing-page-2026-09-09.html', import.meta.url);
const FIXTURE_PATH = fileURLToPath(FIXTURE_URL);
// 이전 동결 fixture(2026-07-05): 5.1 계열·Opus 5 행이 없다. 회귀 방지용으로 유지.
const LEGACY_FIXTURE_URL = new URL('../tests/fixtures/pricing/real/pricing-page-2026-07-05.html', import.meta.url);
const PRICING_URL = new URL('../pricing.json', import.meta.url);
const PRICING_PATH = fileURLToPath(PRICING_URL);

const fixture = readFileSync(FIXTURE_URL, 'utf8');
const legacyFixture = readFileSync(LEGACY_FIXTURE_URL, 'utf8');
const pricing: PricingJson = JSON.parse(readFileSync(PRICING_URL, 'utf8'));

/** 2026-09-09 fixture에 없는 모델(이전 fixture 회귀 테스트에서 제외). */
const MODELS_ABSENT_IN_LEGACY = ['claude-fable-5-1', 'claude-mythos-5-1', 'claude-opus-5'];

/**
 * 실 fixture에서 "Claude Fable 5" 표 행(5.1 행이 아닌)의 output 단가(50→99)만 바꾼 drift
 * HTML을 임시 파일로 만든다. 행은 `<tr>` 단위로 텍스트화해 "Claude Fable 5 $10 / MTok"로
 * 시작하는 것을 고른다 — 문자열 순번(n번째 occurrence)은 페이지 개편(5.1 행 추가·nav
 * 링크 변동)에 취약해 버렸다.
 */
function writeDriftFixture(): string {
  const rows = [...fixture.matchAll(/<tr[^>]*>[\s\S]*?<\/tr>/g)];
  const hit = rows.find((m) => /^\s*Claude Fable 5 \$10 \/ MTok/.test(htmlToText(m[0])));
  assert.ok(hit && hit.index !== undefined, 'fixture 표에 "Claude Fable 5 $10 / MTok" 행이 있어야 한다');
  const rowStart = hit.index;
  const rowEnd = rowStart + hit[0].length;
  const row = hit[0];
  assert.equal(row.split('$50 / MTok').length - 1, 1, 'Fable 5 행에 $50 / MTok가 정확히 1개여야 drift 주입이 안전');
  const drifted = fixture.slice(0, rowStart) + row.replace('$50 / MTok', '$99 / MTok') + fixture.slice(rowEnd);
  const dir = mkdtempSync(join(tmpdir(), 'wimcc-pricing-'));
  const p = join(dir, 'drift.html');
  writeFileSync(p, drifted);
  return p;
}

function run(args: string[]) {
  return spawnSync(process.execPath, [SCRIPT, ...args], { encoding: 'utf8' });
}

// ── 파서 in-process 단위 테스트 ────────────────────────────────────────────

test('동결 페이지 파싱 결과가 체크인된 가격표와 정확히 일치한다', () => {
  const parsed = parsePricingPage(fixture, Object.keys(pricing.models));
  assert.deepEqual(diffRates(pricing, parsed), []);
});

test('이전 동결 fixture(2026-07-05)도 그 표에 있는 모델은 가격표와 일치한다(회귀)', () => {
  const ids = Object.keys(pricing.models).filter((id) => !MODELS_ABSENT_IN_LEGACY.includes(id));
  const subset: PricingJson = {
    ...pricing,
    models: Object.fromEntries(ids.map((id) => [id, pricing.models[id]])),
  };
  assert.deepEqual(diffRates(subset, parsePricingPage(legacyFixture, ids)), []);
});

// ── 표시명 접두 충돌 (2026-09-07 자동 PR #118 사고) ─────────────────────────
// 페이지 표에 "Claude Fable 5.1" 행이 "Claude Fable 5" 행보다 앞에 오자 `indexOf('Claude
// Fable 5')`가 5.1 행에 걸려, 5.1의 cache read($0.25)를 claude-fable-5 단가로 기록했다.
// 2026-09-09 동결 fixture가 그 실물이다(각주: 0.025x는 5.1 계열에만, 나머지는 0.1x).

test('접두 충돌: "Claude Fable 5"는 "Claude Fable 5.1" 행이 아니라 자기 행을 읽는다', () => {
  const parsed = parsePricingPage(fixture, ['claude-fable-5', 'claude-mythos-5']);
  assert.equal(parsed['claude-fable-5'].cache_read_per_mtok, 1);
  assert.equal(parsed['claude-mythos-5'].cache_read_per_mtok, 1);
});

test('5.1 계열·Opus 5는 2026-09-09 fixture에서 자기 단가로 읽힌다', () => {
  const parsed = parsePricingPage(fixture, ['claude-fable-5-1', 'claude-mythos-5-1', 'claude-opus-5']);
  assert.deepEqual(parsed['claude-fable-5-1'], {
    input_per_mtok: 10,
    cache_creation_per_mtok: 12.5,
    cache_read_per_mtok: 0.25,
    output_per_mtok: 50,
  });
  assert.deepEqual(parsed['claude-mythos-5-1'], parsed['claude-fable-5-1']);
  assert.deepEqual(parsed['claude-opus-5'], {
    input_per_mtok: 5,
    cache_creation_per_mtok: 6.25,
    cache_read_per_mtok: 0.5,
    output_per_mtok: 25,
  });
});

test('접두 충돌: 표시명 뒤에 소수점 버전이 붙은 행은 짧은 별칭에 매칭되지 않는다(합성)', () => {
  // 짧은 별칭 행이 아예 없으면 throw — 5.1 행을 폴백으로 삼지 않는다.
  const html =
    'Model pricing Claude Fable 5.1 $10 / MTok $12.50 / MTok $20 / MTok $0.25 / MTok $50 / MTok MTok = Million tokens';
  assert.throws(() => parsePricingPage(html, ['claude-fable-5']), /model not found/);
});

test('등록 모델이 페이지에 없으면 throw (구조 변경 신호)', () => {
  assert.throws(() => parsePricingPage('<html><body>empty</body></html>', ['claude-fable-5']));
});

test('표 앵커는 있으나 단가 열이 부족하면 throw (열 개편 신호)', () => {
  // "Model pricing"~"MTok = Million tokens" 앵커는 있고 모델 표기도 있으나 $N/MTok가 5개 미만
  const html = 'Model pricing Claude Fable 5 $10 / MTok $12.50 / MTok MTok = Million tokens';
  assert.throws(() => parsePricingPage(html, ['claude-fable-5']), /expected >=5/);
});

test('단가 변동은 diff 줄로 드러난다', () => {
  const parsed = parsePricingPage(fixture, Object.keys(pricing.models));
  parsed['claude-fable-5'] = { ...parsed['claude-fable-5'], output_per_mtok: 60 };
  const diff = diffRates(pricing, parsed);
  assert.equal(diff.length, 1);
  assert.match(diff[0], /claude-fable-5\.output_per_mtok: 50 -> 60/);
});

// ── 직렬화 포맷 보존 (리뷰 Important 2) ────────────────────────────────────

test('serializePricing은 pricing.json을 바이트 그대로 재현한다(정수 rate .0 유지)', () => {
  assert.equal(serializePricing(pricing), readFileSync(PRICING_URL, 'utf8'));
});

test('formatRate: 정수는 .0 유지, 소수는 최소 표기', () => {
  assert.equal(formatRate(10), '10.0');
  assert.equal(formatRate(12.5), '12.5');
  assert.equal(formatRate(0.3), '0.3');
});

// ── --check 종료코드 계약 (리뷰 Important 1: Task 6 CI 게이트의 유일 의존) ──

test('subprocess --check: 무변동 fixture → exit 0', () => {
  const r = run(['--check', '--source', FIXTURE_PATH]);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /no drift/);
});

test('subprocess --check: drift 주입 fixture → exit 1', () => {
  const drift = writeDriftFixture();
  const r = run(['--check', '--source', drift]);
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stdout, /claude-fable-5\.output_per_mtok: 50 -> 99/);
});

test('subprocess --check: 파싱 실패(빈/깨진 HTML) → exit 1', () => {
  const dir = mkdtempSync(join(tmpdir(), 'wimcc-pricing-'));
  const p = join(dir, 'broken.html');
  writeFileSync(p, '<html><body>no pricing table here</body></html>');
  const r = run(['--check', '--source', p]);
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, /table start anchor not found/);
});

test('subprocess (--check 미지정): drift 있어도 exit 0', () => {
  const drift = writeDriftFixture();
  const r = run(['--source', drift]);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /claude-fable-5\.output_per_mtok: 50 -> 99/);
});

// ── --write 격리 + 포맷 보존 (리뷰 Important 2·3) ──────────────────────────

test('subprocess --write --out: 임시 파일에만 쓰고 실 pricing.json은 불변', () => {
  const before = readFileSync(PRICING_PATH, 'utf8');
  const drift = writeDriftFixture();
  const dir = mkdtempSync(join(tmpdir(), 'wimcc-pricing-'));
  const outPath = join(dir, 'pricing.out.json');
  const r = run(['--write', '--source', drift, '--out', outPath]);
  assert.equal(r.status, 0, r.stderr);

  // 실 파일 clobber 없음
  assert.equal(readFileSync(PRICING_PATH, 'utf8'), before);

  // 변동된 필드 1개 + version 줄만 바뀌고 나머지 라인은 바이트 동일
  const origLines = before.split('\n');
  const outLines = readFileSync(outPath, 'utf8').split('\n');
  assert.equal(origLines.length, outLines.length);
  const changed = origLines.map((l, i) => (l === outLines[i] ? -1 : i)).filter((i) => i >= 0);
  const changedText = changed.map((i) => outLines[i]);
  // 변동 라인은 version 줄 또는 drift 필드(99.0)뿐 — 스퓨리어스 리포맷 없음.
  // version은 --write가 실행일(today)로 세팅하므로, pricing.json이 마침 오늘 갱신돼
  // 있으면 동일값이라 변동 목록에서 빠질 수 있다(당일 idempotent). 그래서 1~2줄을
  // 허용하되, 변동 라인은 반드시 version/drift 중 하나여야 한다(날짜 결합 제거).
  const isVersion = (l: string) => /"version": "pricing_estimate@\d{4}-\d{2}-\d{2}"/.test(l);
  const isDrift = (l: string) => /"output_per_mtok": 99\.0/.test(l);
  assert.ok(
    changed.length >= 1 && changed.length <= 2 && changedText.every((l) => isVersion(l) || isDrift(l)),
    `version/drift 외 변경이 있으면 안 됨: ${changedText.join(' | ')}`,
  );
  assert.ok(changedText.some(isDrift), 'drift 필드가 99.0(정수 .0 유지)로 갱신');
  // 출력 version 줄은 당일 idempotent 여부와 무관하게 항상 형식이 유효해야 한다.
  assert.ok(outLines.some(isVersion), 'version 줄 형식 유효');
  // 변동 안 된 정수 rate는 .0을 그대로 유지(리포맷 노이즈 없음)
  assert.ok(outLines.some((l) => /"input_per_mtok": 10\.0/.test(l)), '10.0 포맷 보존');
  assert.ok(outLines.some((l) => /"cache_read_per_mtok": 1\.0/.test(l)), '1.0 포맷 보존');
});

// ── 신규 모델 탐지 (등록 모델만 순회하던 맹점 보강, 2026-07-05) ──────────────

test('detectNewModels: 표에 등장하나 alias 미등록인 모델 표시명을 surface한다', () => {
  const html =
    'Model pricing Claude Fable 5 $10 / MTok Claude Zephyr 9 $7 / MTok MTok = Million tokens';
  const found = detectNewModels(html, { 'claude-fable-5': ['Claude Fable 5'] });
  assert.deepEqual(found, ['Claude Zephyr 9']);
});

test('detectNewModels: retired/deprecated 행은 노이즈라 제외한다', () => {
  const html = 'Model pricing Claude Zephyr 9 ( retired ) $7 / MTok MTok = Million tokens';
  assert.deepEqual(detectNewModels(html, {}), []);
});

test('detectNewModels: 동결 fixture엔 미등록 활성 모델이 없다(활성 모델 전부 사전 추가)', () => {
  // 신규 모델은 이 목록에 뜨는 즉시 PAGE_ALIASES + pricing.json에 사전 추가되어야 한다.
  assert.deepEqual(detectNewModels(fixture), []);
});
