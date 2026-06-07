/**
 * Pure view-model builder for the insight strip (design spec §3/§5).
 * Turns the already-fetched query DTOs into typed cards with provenance and
 * drill payloads. ALL derivation logic lives here so it is unit-testable in
 * jsdom; the component only renders. Degrades gracefully: when a backend slice
 * has not landed, the relevant card is badged `uncollected` (미수집·예정).
 *
 * Consumes the REAL APIs:
 *  - VerificationRunDto.detection_basis / status_basis drive the 검증
 *    card's badge (measured when all runs are known_tool + exit, else mixed).
 *  - SignalDto(detector=tool_failure) count drives the 도구 실패 card
 *    (deterministic L1 extractor count).
 *  - SessionUsageDto.estimated_cost_usd / cost_basis is the cost 추정.
 *  - An optional baseline (cache_hit_ratio) renders a "vs median" delta.
 */
import type {
  SessionUsageDto,
  VerificationRunDto,
  SignalDto,
} from '../../../api/types';
import { formatPct, formatTokens, formatUsd } from '../../../lib/format';
import type { Provenance } from './provenance';

/** Optional cross-session baseline (slice 6). Absent today → no delta shown. */
export interface InsightBaseline {
  cache_hit_ratio?: number | null;
}

export interface InsightInputs {
  usage: SessionUsageDto | undefined;
  verificationRuns: VerificationRunDto[] | undefined;
  signals: SignalDto[] | undefined;
  baseline?: InsightBaseline;
}

export type InsightCardId =
  | 'context'
  | 'tokens'
  | 'verification'
  | 'tool_failure'
  | 'cost';

export interface InsightCardModel {
  id: InsightCardId;
  /** Korean card title shown in the strip. */
  title: string;
  /** Headline value, already formatted; `—` when uncollected. */
  value: string;
  /** One-line micro-detail under the value. */
  detail: string;
  provenance: Provenance;
  /** Long-form text for the `?` tooltip. */
  tooltip: string;
  /** Inline drill content shown when the card is expanded. */
  drill?: {
    lines: string[];
    byKind?: Record<string, number>;
  };
  /** Optional "vs your median" delta (slice 6); undefined when no baseline. */
  baselineDelta?: string;
}

const GUARD_KIND: Record<string, 'test' | 'build' | 'lint' | 'format'> = {
  test_suite_js: 'test',
  test_suite_rust: 'test',
  test_suite_py: 'test',
  test_suite_go: 'test',
  test_suite_java: 'test',
  build: 'build',
  build_check: 'build',
  lint: 'lint',
  format_check: 'format',
};

function contextCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '캐시 적중률 = cache_read / (cache_read + cache_creation + input). 측정값(usage facet). ' +
    '고정 캐시 컨텍스트 크기·증가·캐시 미스는 펼쳐서 확인. 시스템 프롬프트/스킬/메모리 단위 분해와 ' +
    '"오염" 판정은 데이터에 없어 제공하지 않습니다(설계 §8 한계).';
  if (!inputs.usage) {
    return {
      id: 'context', title: '컨텍스트 효율', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const card: InsightCardModel = {
    id: 'context', title: '컨텍스트 효율',
    value: formatPct(u.cache_hit_ratio),
    detail: `캐시 읽기 ${formatTokens(u.cache_read_input_tokens)}`,
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `캐시 적중률 ${formatPct(u.cache_hit_ratio)}`,
        `캐시 읽기(무료) ${formatTokens(u.cache_read_input_tokens)}`,
        `캐시 생성 ${formatTokens(u.cache_creation_input_tokens)}`,
        `턴 수 ${u.turns}`,
      ],
    },
  };
  const base = inputs.baseline?.cache_hit_ratio;
  if (typeof base === 'number' && typeof u.cache_hit_ratio === 'number') {
    const d = Math.round((u.cache_hit_ratio - base) * 100);
    card.baselineDelta = `${d >= 0 ? '+' : ''}${d}%p vs 중앙값`;
  }
  return card;
}

function tokensCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '청구 토큰(input + cache_creation + output)과 캐시 읽기(무료)는 의미가 달라 절대 합산하지 않습니다 ' +
    '(설계 §3 Q2). 측정값(usage facet).';
  if (!inputs.usage) {
    return {
      id: 'tokens', title: '토큰', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  return {
    id: 'tokens', title: '토큰',
    value: `청구 ${formatTokens(u.billed_tokens)}`,
    detail: `캐시 읽기 ${formatTokens(u.cache_read_input_tokens)} (무료)`,
    provenance: 'measured', tooltip: tip,
    drill: {
      lines: [
        `input ${formatTokens(u.input_tokens)}`,
        `cache_creation ${formatTokens(u.cache_creation_input_tokens)}`,
        `output ${formatTokens(u.output_tokens)}`,
        ...u.by_model.map((m) => `${m.model}: ${m.turns}턴 · 출력 ${formatTokens(m.output_tokens)}`),
      ],
    },
  };
}

function verificationCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '가드 = 실행된 테스트/빌드/린트/포맷 검사. 알려진 도구 매칭(known_tool) + 종료코드(exit) 기반이면 측정, ' +
    '키워드 추정(test_keyword)이나 파이프(piped)로 가려진 종료코드가 섞이면 혼합으로 표시(슬라이스 2 ' +
    'detection_basis/status_basis). 브라우저 스모크/서브에이전트 테스트는 감지하지 않습니다(설계 §3 Q4 한계).';
  const runs = inputs.verificationRuns;
  if (!runs || runs.length === 0) {
    return {
      id: 'verification', title: '검증', value: '—',
      detail: runs ? '감지된 가드 없음' : '로딩 중',
      provenance: 'uncollected', tooltip: tip,
    };
  }
  const byKind: Record<string, number> = {};
  let passed = 0;
  let allMeasured = true;
  for (const r of runs) {
    const k = GUARD_KIND[r.command_kind] ?? 'test';
    byKind[k] = (byKind[k] ?? 0) + 1;
    if (r.status === 'passed') passed += 1;
    // slice-2 fields: measured only when every run is a known-tool match with a
    // direct exit-code status. Keyword guesses or piped (masked) exits → mixed.
    if (r.detection_basis !== 'known_tool' || r.status_basis !== 'exit') {
      allMeasured = false;
    }
  }
  return {
    id: 'verification', title: '검증',
    value: `가드 ${runs.length} · 통과 ${passed}`,
    detail: Object.entries(byKind).map(([k, n]) => `${k} ${n}`).join(' · '),
    provenance: allMeasured ? 'measured' : 'mixed', tooltip: tip,
    drill: {
      lines: runs.map((r) => `${r.command_kind} → ${r.status}`),
      byKind,
    },
  };
}

function toolFailureCard(inputs: InsightInputs): InsightCardModel {
  const tip = '도구 실패 signal 수(detector=tool_failure). 결정적 카운트이며 심각도 판단은 포함하지 않습니다.';
  const sigs = inputs.signals;
  if (!sigs) {
    return { id: 'tool_failure', title: '도구 실패', value: '—', detail: '로딩 중', provenance: 'uncollected', tooltip: tip };
  }
  const failures = sigs.filter((s) => s.detector === 'tool_failure');
  return {
    id: 'tool_failure', title: '도구 실패',
    value: `${failures.length}`,
    detail: failures.length === 0 ? '도구 실패 없음' : '펼쳐서 확인',
    provenance: 'measured', tooltip: tip,
    drill: { lines: failures.map((s) => `${s.subkind ?? s.detector} · ${s.summary}`) },
  };
}

function costCard(inputs: InsightInputs): InsightCardModel {
  const tip =
    '공개 가격표 × usage 토큰으로 계산한 추정치이며 실제 청구액이 아닙니다(설계 §6.5/§11.3). ' +
    'OTel claude_code.cost.usage 메트릭이 들어오면 대체됩니다. cache_read(무료)는 비용에서 제외.';
  if (!inputs.usage) {
    return {
      id: 'cost', title: '비용', value: '—',
      detail: 'usage facet 재수집 필요', provenance: 'uncollected', tooltip: tip,
    };
  }
  const u = inputs.usage;
  const unpriced = u.models_without_pricing.length > 0;
  return {
    id: 'cost', title: '비용',
    value: formatUsd(u.estimated_cost_usd),
    detail: unpriced
      ? `공개 가격표 추정 (≈) · 미가격 ${u.models_without_pricing.length}`
      : '공개 가격표 추정 (≈)',
    provenance: 'estimated', tooltip: tip,
    drill: {
      lines: u.by_model.map(
        (m) => `${m.model}: ${m.priced ? formatUsd(m.estimated_cost_usd) : '가격표 없음'}`,
      ),
    },
  };
}

export function buildInsightCards(inputs: InsightInputs): InsightCardModel[] {
  return [
    contextCard(inputs),
    tokensCard(inputs),
    verificationCard(inputs),
    toolFailureCard(inputs),
    costCard(inputs),
  ];
}
