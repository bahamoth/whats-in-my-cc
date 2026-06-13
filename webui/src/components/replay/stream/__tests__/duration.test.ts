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
  it('rounds the whole duration so the seconds field never reaches 60 (bug: 1m 60s)', () => {
    // 119_500ms rounds to 120s = 2m 0s, NOT 1m 60s. Rounding the remainder
    // seconds independently of the minutes carries 60 into the seconds slot.
    expect(formatDuration(119_500)).toBe('2m 0s');
    expect(formatDuration(179_800)).toBe('3m 0s');
    // a clean minute and a mid-minute value must stay correct after the fix
    expect(formatDuration(60_000)).toBe('1m 0s');
    expect(formatDuration(90_400)).toBe('1m 30s');
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
