import { describe, it, expect } from 'vitest';
import { formatDuration } from '../duration';

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
