import { describe, expect, it } from 'vitest';
import {
  gateVerdict,
  INTENTIONALLY_UNMATCHED_MCP,
} from '../taggingGate';

// B-7d — PR-전 태깅 게이트의 순수 판정. 원칙:
// - untagged 토큰 count>=2 는 "보편 후보 잔존" 추정 → 사전 추가 또는
//   baseline 보류(사유 필수) 전에는 차단.
// - official/public plugin 의 unmatched MCP 도구는 차단 — 단 기결정
//   (intentionally unmatched, B-7c) 목록은 매 루프 재표면화하지 않는다.
describe('gateVerdict', () => {
  it('passes a clean state', () => {
    const v = gateVerdict({ untagged: [], unidentified: [] }, new Set());
    expect(v.pass).toBe(true);
    expect(v.failures).toEqual([]);
  });

  it('fails on an untagged token with count >= 2 not in the baseline', () => {
    const v = gateVerdict(
      { untagged: [{ token: 'frobnicate', count: 3 }], unidentified: [] },
      new Set(),
    );
    expect(v.pass).toBe(false);
    expect(v.failures[0]).toMatchObject({ kind: 'untagged', token: 'frobnicate' });
  });

  it('lets count-1 stragglers and baselined tokens through', () => {
    const v = gateVerdict(
      {
        untagged: [
          { token: 'one-off', count: 1 },
          { token: 'known-fragment', count: 5 },
        ],
        unidentified: [],
      },
      new Set(['known-fragment']),
    );
    expect(v.pass).toBe(true);
  });

  it('fails on an unmatched official/public MCP tool', () => {
    const v = gateVerdict(
      {
        untagged: [],
        unidentified: [{ token: 'context7:new-tool', provenance: 'official' }],
      },
      new Set(),
    );
    expect(v.pass).toBe(false);
    expect(v.failures[0]).toMatchObject({ kind: 'mcp', token: 'context7:new-tool' });
  });

  it('skips intentionally-unmatched MCP tools (B-7c) and non-community provenance', () => {
    const v = gateVerdict(
      {
        untagged: [],
        unidentified: [
          { token: 'serena:activate_project', provenance: 'official' },
          { token: 'optiflow-help:whoami', provenance: 'configured' },
        ],
      },
      new Set(),
    );
    expect(v.pass).toBe(true);
  });

  it('exposes the decided list for the loop scripts', () => {
    expect(INTENTIONALLY_UNMATCHED_MCP.has('serena:activate_project')).toBe(true);
    expect(INTENTIONALLY_UNMATCHED_MCP.has('serena:onboarding')).toBe(true);
  });
});
