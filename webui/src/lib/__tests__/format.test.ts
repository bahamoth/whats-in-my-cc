/**
 * PR-3 RED — format helpers. Locked behaviour:
 *  - ms uses `Xs`, `Xm Ys`, `Xh Ym`. 0 ms returns "0ms" (no unit collapse).
 *  - usd shows 2+ significant digits below $1, then dollars with up to 2 dp.
 *  - tokens use `k` and `M` suffixes with one decimal.
 *  - pct rounds to integer.
 *  - bytes use binary IEC (KiB / MiB / GiB).
 *  - All formatters return "—" for null / undefined / NaN / negative.
 *
 * See plan §10.1 PR-3.
 */
import { describe, expect, it } from 'vitest';
import { formatMs, formatUsd, formatTokens, formatPct, formatBytes } from '../format';

describe('formatMs', () => {
  it('renders sub-second as Xms', () => {
    expect(formatMs(0)).toBe('0ms');
    expect(formatMs(42)).toBe('42ms');
    expect(formatMs(999)).toBe('999ms');
  });
  it('renders 1s+ as Xs', () => {
    expect(formatMs(1000)).toBe('1.0s');
    expect(formatMs(1500)).toBe('1.5s');
    expect(formatMs(59_999)).toBe('60.0s');
  });
  it('renders 1m+ as Xm Ys', () => {
    expect(formatMs(60_000)).toBe('1m 0s');
    expect(formatMs(123_456)).toBe('2m 3s');
  });
  it('renders 1h+ as Xh Ym', () => {
    expect(formatMs(3_600_000)).toBe('1h 0m');
    expect(formatMs(3_900_000)).toBe('1h 5m');
  });
  it('returns placeholder for invalid input', () => {
    expect(formatMs(null)).toBe('—');
    expect(formatMs(undefined)).toBe('—');
    expect(formatMs(NaN)).toBe('—');
    expect(formatMs(-1)).toBe('—');
  });
});

describe('formatUsd', () => {
  it('uses 4 decimal places below $1', () => {
    expect(formatUsd(0.0042)).toBe('$0.0042');
    expect(formatUsd(0.5)).toBe('$0.5000');
  });
  it('uses 2 decimal places at or above $1', () => {
    expect(formatUsd(1)).toBe('$1.00');
    expect(formatUsd(12.345)).toBe('$12.35');
    expect(formatUsd(1234.5)).toBe('$1,234.50');
  });
  it('returns placeholder for invalid input', () => {
    expect(formatUsd(null)).toBe('—');
    expect(formatUsd(undefined)).toBe('—');
    expect(formatUsd(NaN)).toBe('—');
  });
});

describe('formatTokens', () => {
  it('renders raw counts below 1k', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(999)).toBe('999');
  });
  it('renders k for thousands', () => {
    expect(formatTokens(1_500)).toBe('1.5k');
    expect(formatTokens(15_432)).toBe('15.4k');
  });
  it('renders M for millions', () => {
    expect(formatTokens(1_500_000)).toBe('1.5M');
  });
  it('returns placeholder for invalid input', () => {
    expect(formatTokens(null)).toBe('—');
    expect(formatTokens(NaN)).toBe('—');
  });
});

describe('formatPct', () => {
  it('renders 0..1 as integer percent', () => {
    expect(formatPct(0)).toBe('0%');
    expect(formatPct(0.873)).toBe('87%');
    expect(formatPct(1)).toBe('100%');
  });
  it('returns placeholder for invalid input', () => {
    expect(formatPct(null)).toBe('—');
    expect(formatPct(NaN)).toBe('—');
    expect(formatPct(-0.1)).toBe('—');
  });
});

describe('formatBytes', () => {
  it('renders bytes / KiB / MiB / GiB', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1023)).toBe('1023 B');
    expect(formatBytes(1024)).toBe('1.0 KiB');
    expect(formatBytes(1_048_576)).toBe('1.0 MiB');
    expect(formatBytes(1_073_741_824)).toBe('1.0 GiB');
  });
  it('returns placeholder for invalid input', () => {
    expect(formatBytes(null)).toBe('—');
    expect(formatBytes(NaN)).toBe('—');
    expect(formatBytes(-1)).toBe('—');
  });
});
