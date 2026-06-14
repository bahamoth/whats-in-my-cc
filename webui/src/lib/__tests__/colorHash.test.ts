import { describe, it, expect } from 'vitest';
import { agentColor, AGENT_PALETTE } from '../colorHash';

describe('agentColor', () => {
  it('is deterministic: same id → same color', () => {
    expect(agentColor('aa1844')).toBe(agentColor('aa1844'));
  });
  it('maps into the fixed palette', () => {
    expect(AGENT_PALETTE).toContain(agentColor('aa1844'));
    expect(AGENT_PALETTE.length).toBeGreaterThanOrEqual(6);
  });
  it('different ids generally differ (no single-bucket collapse)', () => {
    const ids = ['aa1844', '7b20e4', 'c4d8a0', 'deadbe', '012345', 'fffabc'];
    const colors = new Set(ids.map(agentColor));
    expect(colors.size).toBeGreaterThanOrEqual(4);
  });
  it('null / empty → neutral subtle var (not a palette hue)', () => {
    expect(agentColor(null)).toBe('var(--wimcc-fg-subtle, #6a7180)');
    expect(agentColor('')).toBe('var(--wimcc-fg-subtle, #6a7180)');
  });
});
