#!/usr/bin/env node
/**
 * update-pricing — 공식 공개 가격 페이지를 파싱해 pricing.json과 대조/갱신
 * (스펙 docs/specs/2026-07-04-session-detail-improvements.md §2.4).
 *
 * Usage (repo root, Node 22+ — native TS type-stripping):
 *   node scripts/update-pricing.ts                         # fetch+diff만 출력, exit 0(에러 없으면 변동 무관)
 *   node scripts/update-pricing.ts --check                 # diff 출력 + 변동 있으면 exit 1(CI 게이트)
 *   node scripts/update-pricing.ts --check --markdown      # diff를 마크다운 표로(PR 본문)
 *   node scripts/update-pricing.ts --write                 # 변동 시 pricing.json 재작성, exit 0
 *   node scripts/update-pricing.ts --write --out /tmp/p.json  # 재작성 목적지 지정(테스트 격리용)
 *   node scripts/update-pricing.ts --check --source tests/fixtures/pricing/real/pricing-page-2026-07-05.html
 *   node scripts/update-pricing.ts --source https://example.com/pricing.html
 *
 * exit 0: 성공, (--check 미지정 시) 변동 유무 무관 · exit 1: fetch/파싱 실패(등록 모델
 * 누락 포함) 또는 (--check 지정 시) 변동(drift) 발견.
 * wimcc 바이너리는 런타임에 절대 외부 fetch하지 않는다 — 이 스크립트(개발/CI)만.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export type RatesJson = {
  input_per_mtok: number;
  cache_creation_per_mtok: number;
  cache_read_per_mtok: number;
  output_per_mtok: number;
};
export type PricingJson = {
  version: string;
  source_url: string;
  models: Record<string, RatesJson>;
};

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const PRICING_PATH = join(ROOT, 'pricing.json');

/**
 * 모델 id → 페이지 상 표기 후보(우선순위 순).
 * 2026-07-05 동결 fixture(tests/fixtures/pricing/real/pricing-page-2026-07-05.html)
 * 관측: 페이지는 원본 모델 id 문자열을 쓰지 않고 표시명만 쓴다("Claude Fable 5" 등).
 * 원본 id 후보는 향후 페이지가 id를 노출할 가능성에 대비한 폴백으로 남겨둔다.
 */
const PAGE_ALIASES: Record<string, string[]> = {
  'claude-fable-5': ['claude-fable-5', 'Claude Fable 5'],
  'claude-mythos-5': ['claude-mythos-5', 'Claude Mythos 5'],
  'claude-opus-4-8': ['claude-opus-4-8', 'Claude Opus 4.8'],
  'claude-opus-4-7': ['claude-opus-4-7', 'Claude Opus 4.7'],
  'claude-sonnet-4-6': ['claude-sonnet-4-6', 'Claude Sonnet 4.6'],
  'claude-haiku-4-5-20251001': ['claude-haiku-4-5-20251001', 'claude-haiku-4-5', 'Claude Haiku 4.5'],
};

/**
 * 가격 표 영역의 시작/끝 앵커. 동결 fixture 관측: "Model pricing" 표제가 페이지 안에
 * 여러 번 나오는데(사이드바 TOC 등) **첫 occurrence가 실제 표 표제**이고, 모델 표시명
 * 자체는 표 앞의 nav 링크("Introducing Claude Fable 5 and Claude Mythos 5" 등)에도
 * 나와 첫 occurrence만으로는 표 안 값과 헷갈린다. 그래서 표 시작~끝 구간으로 검색
 * 범위를 좁힌 뒤에만 모델 표시명을 찾는다. 두 앵커 중 하나라도 없으면(페이지 개편)
 * throw — 침묵 실패 금지.
 */
const TABLE_START_MARKER = 'Model pricing';
const TABLE_END_MARKER = 'MTok = Million tokens';

/** HTML → 공백 정규화 텍스트. */
export function htmlToText(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style[\s\S]*?<\/style>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&nbsp;/g, ' ')
    .replace(/\s+/g, ' ');
}

/**
 * 모델 표기 등장 지점 이후 창(window)에서 `$N / MTok` 값을 뽑는다. 공식 표의 열 순서는
 * [Base Input, 5m Cache Write, 1h Cache Write, Cache Hits & Refreshes, Output] 5열
 * (동결 fixture 관측, 2026-07-05) — pricing.json 스키마는 4개 rate만 가지므로
 * "1h Cache Write"(index 2)는 버리고 [input, cache_creation(=5m write), cache_read
 * (=Cache Hits & Refreshes), output]만 취한다. 등록 모델이 하나라도 안 보이거나 표
 * 앵커 자체가 없으면(페이지 개편) throw.
 */
export function parsePricingPage(html: string, modelIds: string[]): Record<string, RatesJson> {
  const text = htmlToText(html);

  const tableStart = text.indexOf(TABLE_START_MARKER);
  if (tableStart < 0) {
    throw new Error(`table start anchor not found on page: "${TABLE_START_MARKER}" (structure changed?)`);
  }
  const tableEnd = text.indexOf(TABLE_END_MARKER, tableStart);
  if (tableEnd < 0) {
    throw new Error(`table end anchor not found on page: "${TABLE_END_MARKER}" (structure changed?)`);
  }
  const table = text.slice(tableStart, tableEnd);

  const out: Record<string, RatesJson> = {};
  for (const id of modelIds) {
    const aliases = PAGE_ALIASES[id] ?? [id];
    const at = aliases.map((a) => table.indexOf(a)).find((i) => i >= 0);
    if (at === undefined) throw new Error(`model not found in pricing table: ${id} (aliases: ${aliases.join(', ')})`);
    const window = table.slice(at, at + 600);
    const dollars = [...window.matchAll(/\$\s*([0-9]+(?:\.[0-9]+)?)\s*\/\s*MTok/gi)].map((m) =>
      Number(m[1]),
    );
    if (dollars.length < 5) {
      throw new Error(
        `expected >=5 "$N / MTok" rates (input, 5m cache write, 1h cache write, cache read, output) near ${id}, got ${dollars.length}`,
      );
    }
    const [input, cacheWrite5m, , cacheRead, output] = dollars;
    out[id] = {
      input_per_mtok: input,
      cache_creation_per_mtok: cacheWrite5m,
      cache_read_per_mtok: cacheRead,
      output_per_mtok: output,
    };
  }
  return out;
}

const RATE_KEYS = [
  'input_per_mtok',
  'cache_creation_per_mtok',
  'cache_read_per_mtok',
  'output_per_mtok',
] as const;

/** 등록 모델 한정 비교 — 변경 줄 목록(빈 배열 = 무변동). */
export function diffRates(current: PricingJson, parsed: Record<string, RatesJson>): string[] {
  const changes: string[] = [];
  for (const [id, cur] of Object.entries(current.models)) {
    const next = parsed[id];
    for (const k of RATE_KEYS) {
      if (next[k] !== cur[k]) changes.push(`${id}.${k}: ${cur[k]} -> ${next[k]}`);
    }
  }
  return changes;
}

/**
 * rate 숫자를 pricing.json 관례대로 포맷한다: 정수는 `.0`을 유지(10 → "10.0"),
 * 그 외는 최소 소수 표기(12.5 → "12.5"). `JSON.stringify`는 10.0을 "10"으로
 * 재포맷해 변동 안 된 필드까지 diff에 노이즈로 실리므로(리뷰 Important 2) 쓰지 않는다.
 */
export function formatRate(v: number): string {
  return Number.isInteger(v) ? `${v}.0` : String(v);
}

/**
 * pricing.json을 원본 바이트 포맷(2-space indent·정수 rate `.0` 유지·후행 개행)
 * 그대로 재직렬화한다. 변동된 (model,field)만 diff에 나오고 나머지 라인은 바이트
 * 동일하게 유지된다 — `serializePricing(원본) === 원본 텍스트`가 성립(테스트가 잠금).
 */
export function serializePricing(p: PricingJson): string {
  const models = Object.entries(p.models)
    .map(([id, r]) => {
      const fields = RATE_KEYS.map((k) => `      ${JSON.stringify(k)}: ${formatRate(r[k])}`).join(
        ',\n',
      );
      return `    ${JSON.stringify(id)}: {\n${fields}\n    }`;
    })
    .join(',\n');
  return (
    '{\n' +
    `  "version": ${JSON.stringify(p.version)},\n` +
    `  "source_url": ${JSON.stringify(p.source_url)},\n` +
    '  "models": {\n' +
    `${models}\n` +
    '  }\n' +
    '}\n'
  );
}

function toMarkdown(changes: string[], sourceUrl: string): string {
  const rows = changes.map((c) => {
    const m = c.match(/^(.+)\.(\w+): (.+) -> (.+)$/)!;
    return `| \`${m[1]}\` | ${m[2]} | $${m[3]} | $${m[4]} |`;
  });
  return [
    '## 공개 가격표 변동',
    '',
    `출처: ${sourceUrl}`,
    '',
    '| model | rate | before | after |',
    '|---|---|---|---|',
    ...rows,
    '',
  ].join('\n');
}

async function fetchSource(src: string): Promise<string> {
  if (/^https?:\/\//.test(src)) {
    const res = await fetch(src, {
      headers: { 'user-agent': 'wimcc-pricing-refresh (github.com/bahamoth/whats-in-my-cc)' },
    });
    if (!res.ok) throw new Error(`fetch ${src}: HTTP ${res.status}`);
    return res.text();
  }
  const path = isAbsolute(src) ? src : join(ROOT, src);
  return readFileSync(path, 'utf8');
}

async function main(): Promise<number> {
  const args = process.argv.slice(2);
  const check = args.includes('--check');
  const write = args.includes('--write');
  const markdown = args.includes('--markdown');
  const srcIdx = args.indexOf('--source');
  const outIdx = args.indexOf('--out');
  // --write 목적지(기본 = repo pricing.json). 테스트가 실 파일을 clobber하지 않도록
  // 임시 경로로 격리할 수 있게 한다(리뷰 Important 3).
  const outPath = outIdx >= 0 ? (isAbsolute(args[outIdx + 1]) ? args[outIdx + 1] : join(ROOT, args[outIdx + 1])) : PRICING_PATH;

  const current: PricingJson = JSON.parse(readFileSync(PRICING_PATH, 'utf8'));
  const html = srcIdx >= 0 ? await fetchSource(args[srcIdx + 1]) : await fetchSource(current.source_url);

  const parsed = parsePricingPage(html, Object.keys(current.models));
  const changes = diffRates(current, parsed);

  if (markdown) {
    console.log(changes.length ? toMarkdown(changes, current.source_url) : '변동 없음.');
  } else {
    console.log(changes.length ? changes.join('\n') : 'no drift');
  }

  if (write && changes.length) {
    // 변동된 필드만 parsed에서 취하고 나머지는 current 값을 그대로 유지 →
    // serializePricing이 원본 포맷을 보존하므로 변경 라인만 diff에 남는다.
    const next: PricingJson = {
      ...current,
      version: `pricing_estimate@${new Date().toISOString().slice(0, 10)}`,
      models: Object.fromEntries(
        Object.entries(current.models).map(([id, cur]) => [
          id,
          Object.fromEntries(RATE_KEYS.map((k) => [k, parsed[id][k]])) as RatesJson,
        ]),
      ),
    };
    writeFileSync(outPath, serializePricing(next));
    console.error(`pricing.json updated: ${next.version} -> ${outPath}`);
  }

  if (check && changes.length) return 1;
  return 0;
}

// node --test가 import할 때는 실행하지 않는다.
if (process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  main()
    .then((code) => process.exit(code))
    .catch((e) => {
      console.error(String(e));
      process.exit(1);
    });
}
