import { describe, it, expect } from 'vitest';
import { formatDuration, durationHeat, HEAT_WARN_MS, HEAT_HOT_MS } from '../duration';

describe('formatDuration', () => {
  it('sub-second stays in ms', () => {
    expect(formatDuration(507)).toBe('507ms');
  });
  it('seconds render with one decimal', () => {
    expect(formatDuration(12_240)).toBe('12.2s');
    expect(formatDuration(42_000)).toBe('42.0s');
  });
  it('a minute or more renders as Nm Ss (long-running work must read at a glance)', () => {
    expect(formatDuration(65_000)).toBe('1m 5s');
    expect(formatDuration(312_000)).toBe('5m 12s');
  });
});

describe('durationHeat — 오래 걸린 작업의 색상 차등 티어', () => {
  it('short executions carry no heat', () => {
    expect(durationHeat(0)).toBeNull();
    expect(durationHeat(HEAT_WARN_MS - 1)).toBeNull();
  });
  it('10s+ is warn, 60s+ is hot', () => {
    expect(HEAT_WARN_MS).toBe(10_000);
    expect(HEAT_HOT_MS).toBe(60_000);
    expect(durationHeat(HEAT_WARN_MS)).toBe('warn');
    expect(durationHeat(HEAT_HOT_MS - 1)).toBe('warn');
    expect(durationHeat(HEAT_HOT_MS)).toBe('hot');
    expect(durationHeat(10 * 60_000)).toBe('hot');
  });
  it('null/undefined duration carries no heat', () => {
    expect(durationHeat(null)).toBeNull();
    expect(durationHeat(undefined)).toBeNull();
  });
});
