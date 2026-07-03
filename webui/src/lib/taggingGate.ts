// B-7c/B-7d (2026-07-04) — PR-전 태깅 게이트의 순수 판정 + 기결정 목록.
//
// 게이트 철학: Noise disposition(2026-06-30)으로 untagged가 진짜 후보만
// 남게 됐고, B-7a 서브셸 인식·무확장 규칙으로 파편 클래스가 닫혔다 — 이제
// "보편 후보 잔존"을 기계 판정으로 차단할 수 있다. 판정은 순수 함수
// (vitest 잠금), 데이터 수집은 scripts/tagging-gate.ts 몫.
//
// baseline: 커밋된 보류 토큰 목록(scripts/tagging-gate-baseline.json —
// 토큰 → 보류 사유). 보류는 PR 리뷰에서 보이는 편집이 된다("이 PR과
// 무관"은 보류 사유가 아니다 — CLAUDE.md 개선 루프).

/** 의도적 unmatched MCP 도구 (B-7c) — Rust SERENA_TOOLS 주석(2026-06-25/26)이
 *  결정 SSOT: activate_project(세션 셋업·상태 write)/onboarding(메타)은
 *  code/file/docs 어느 verb에도 맞지 않아 태깅하지 않는다. 루프 스크립트와
 *  게이트가 이 목록을 인지해 매 루프 재표면화하지 않는다. */
export const INTENTIONALLY_UNMATCHED_MCP: ReadonlySet<string> = new Set([
  'serena:activate_project',
  'serena:onboarding',
]);

export interface GateInputs {
  /** untagged-bash 수집 결과의 (token, count) 축약. */
  untagged: { token: string; count: number }[];
  /** unidentified-plugins 수집 결과의 (token, provenance) 축약. */
  unidentified: { token: string; provenance: string }[];
}

export interface GateFailure {
  kind: 'untagged' | 'mcp';
  token: string;
  detail: string;
}

export interface GateVerdict {
  pass: boolean;
  failures: GateFailure[];
}

/** count 2 이상이면 일회성 파편이 아니라 반복 사용 도구일 개연성 —
 *  "보편 후보 잔존" 추정 임계. */
const UNTAGGED_FAIL_COUNT = 2;

export function gateVerdict(inputs: GateInputs, baseline: ReadonlySet<string>): GateVerdict {
  const failures: GateFailure[] = [];
  for (const u of inputs.untagged) {
    if (u.count >= UNTAGGED_FAIL_COUNT && !baseline.has(u.token)) {
      failures.push({
        kind: 'untagged',
        token: u.token,
        detail: `count ${u.count} — 사전 추가(승인+TDD) 또는 baseline 보류(사유 필수)`,
      });
    }
  }
  for (const m of inputs.unidentified) {
    const community = m.provenance === 'official' || m.provenance === 'public';
    if (community && !INTENTIONALLY_UNMATCHED_MCP.has(m.token)) {
      failures.push({
        kind: 'mcp',
        token: m.token,
        detail: `${m.provenance} plugin 도구 미태깅 — MCP_SERVER_TOOL_TAGS 추가 필요`,
      });
    }
  }
  return { pass: failures.length === 0, failures };
}
