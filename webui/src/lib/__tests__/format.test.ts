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
import {
  formatMs,
  formatUsd,
  formatTokens,
  formatPct,
  formatBytes,
  relativeTime,
  formatModel,
  formatDurationSpan,
  spanMs,
  formatSpan,
} from '../format';

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

// S6 (UX 재설계) — relative time for the session list "last seen" column.
// l10n — the wording is locale-aware; the caller injects the active locale.
describe('relativeTime', () => {
  const now = Date.parse('2026-06-15T12:00:00Z');

  describe('Korean (ko)', () => {
    it('renders just-now under a minute as 방금', () => {
      expect(relativeTime('2026-06-15T11:59:40Z', now, 'ko')).toBe('방금');
    });
    it('renders minutes', () => {
      expect(relativeTime('2026-06-15T11:57:00Z', now, 'ko')).toBe('3분 전');
    });
    it('renders hours', () => {
      expect(relativeTime('2026-06-15T10:00:00Z', now, 'ko')).toBe('2시간 전');
    });
    it('renders days', () => {
      expect(relativeTime('2026-06-13T12:00:00Z', now, 'ko')).toBe('2일 전');
    });
    it('falls back to a date for older timestamps', () => {
      expect(relativeTime('2026-04-01T12:00:00Z', now, 'ko')).toBe('2026-04-01');
    });
  });

  describe('English (en)', () => {
    it('renders just-now under a minute', () => {
      expect(relativeTime('2026-06-15T11:59:40Z', now, 'en')).toBe('just now');
    });
    it('renders minutes', () => {
      expect(relativeTime('2026-06-15T11:57:00Z', now, 'en')).toBe('3 min ago');
    });
    it('renders hours', () => {
      expect(relativeTime('2026-06-15T10:00:00Z', now, 'en')).toBe('2 hr ago');
    });
    it('renders days, pluralizing past one', () => {
      expect(relativeTime('2026-06-14T12:00:00Z', now, 'en')).toBe('1 day ago');
      expect(relativeTime('2026-06-13T12:00:00Z', now, 'en')).toBe('2 days ago');
    });
    it('falls back to a date for older timestamps', () => {
      expect(relativeTime('2026-04-01T12:00:00Z', now, 'en')).toBe('2026-04-01');
    });
  });

  it('defaults to English when no locale is given', () => {
    expect(relativeTime('2026-06-15T11:57:00Z', now)).toBe('3 min ago');
  });

  it('returns placeholder for invalid input', () => {
    expect(relativeTime('not-a-date', now, 'en')).toBe('—');
    expect(relativeTime('', now, 'ko')).toBe('—');
  });
});

// S6 — humanise the raw model id for the session list / KPI tags.
describe('formatModel', () => {
  it('humanises opus / sonnet / haiku ids', () => {
    expect(formatModel('claude-opus-4-8')).toBe('Opus 4.8');
    expect(formatModel('claude-sonnet-4-6')).toBe('Sonnet 4.6');
    expect(formatModel('claude-haiku-4-5-20251001')).toBe('Haiku 4.5');
    // minor 없는 이름 — displayModel 위임으로 고쳐진 회귀(리스트에 원문 노출됐었음)
    expect(formatModel('claude-fable-5')).toBe('Fable 5');
  });
  it('strips a [1m] context suffix', () => {
    expect(formatModel('claude-opus-4-8[1m]')).toBe('Opus 4.8');
  });
  it('returns the raw id when it does not match the known shape', () => {
    expect(formatModel('gpt-4o')).toBe('gpt-4o');
  });
  it('returns placeholder for empty / nullish', () => {
    expect(formatModel(null)).toBe('—');
    expect(formatModel(undefined)).toBe('—');
    expect(formatModel('')).toBe('—');
  });
});

describe('formatDurationSpan', () => {
  it('sub-minute → whole seconds', () => {
    expect(formatDurationSpan(45_000)).toBe('45s');
  });
  it('minutes only (drops seconds)', () => {
    expect(formatDurationSpan(8 * 60_000 + 30_000)).toBe('8m');
  });
  it('hours + minutes', () => {
    expect(formatDurationSpan(5 * 3_600_000 + 12 * 60_000)).toBe('5h 12m');
  });
  it('days + hours for a 2-day+ long session', () => {
    expect(formatDurationSpan(2 * 86_400_000 + 3 * 3_600_000)).toBe('2d 3h');
  });
  it('negative clamps to 0s (clock skew)', () => {
    expect(formatDurationSpan(-5)).toBe('0s');
  });
  it('nullish / NaN → placeholder', () => {
    expect(formatDurationSpan(null)).toBe('—');
    expect(formatDurationSpan(undefined)).toBe('—');
    expect(formatDurationSpan(NaN)).toBe('—');
  });
});

describe('spanMs / formatSpan', () => {
  it('spanMs computes the ms between two ISO timestamps', () => {
    expect(spanMs('2026-07-03T00:00:00Z', '2026-07-05T03:00:00Z')).toBe(
      2 * 86_400_000 + 3 * 3_600_000,
    );
  });
  it('formatSpan formats the derived span', () => {
    expect(formatSpan('2026-07-03T00:00:00Z', '2026-07-05T03:00:00Z')).toBe('2d 3h');
  });
  it('spanMs is null and formatSpan is placeholder for bad input', () => {
    expect(spanMs(null, '2026-07-05T00:00:00Z')).toBeNull();
    expect(spanMs('nope', 'nope')).toBeNull();
    expect(formatSpan('nope', 'nope')).toBe('—');
  });
});
