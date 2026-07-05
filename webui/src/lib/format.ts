import { displayModel } from './seriesView';
/**
 * PR-3 — display formatters. Always return "—" for null / undefined /
 * NaN / negative values so the UI never shows raw `NaN` or `undefined`.
 */
import type { Locale } from '../i18n/detect';

const PLACEHOLDER = '—';

function bad(n: number | null | undefined): boolean {
  return n === null || n === undefined || Number.isNaN(n) || n < 0;
}

/** Wall-clock HH:MM on the viewer's LOCAL clock — the left time-spine ruler in
 *  the conversation stream. Empty string for null/NaN so the spine just omits
 *  the label (it never shows "—" as a time). */
export function clockLabel(ms: number | null | undefined): string {
  if (bad(ms)) return '';
  const d = new Date(ms as number);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function formatMs(ms: number | null | undefined): string {
  if (bad(ms)) return PLACEHOLDER;
  const v = ms as number;
  if (v < 1000) return `${Math.round(v)}ms`;
  if (v < 60_000) return `${(v / 1000).toFixed(1)}s`;
  if (v < 3_600_000) {
    const m = Math.floor(v / 60_000);
    const s = Math.floor((v - m * 60_000) / 1000);
    return `${m}m ${s}s`;
  }
  const h = Math.floor(v / 3_600_000);
  const m = Math.floor((v - h * 3_600_000) / 60_000);
  return `${h}h ${m}m`;
}

/**
 * Compact wall-clock span for "how long is this session" (session list + detail
 * header, 2026-07-05). Unlike `formatMs` it rolls up into DAYS so a 2-day+ long
 * session reads as `2d 3h`, not `51h 12m`. Seconds/minutes/hours/days, dropping
 * the smaller unit once past it. Negative (clock skew) clamps to `0s`; nullish
 * / NaN → placeholder.
 */
export function formatDurationSpan(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || Number.isNaN(ms)) return PLACEHOLDER;
  const v = Math.max(0, ms);
  const s = Math.floor(v / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${h % 24}h`;
}

/** Milliseconds between two RFC3339/ISO instants (session span = last − first).
 *  `null` when either is missing/unparseable; clamps to ≥0 for clock skew. */
export function spanMs(
  firstIso: string | null | undefined,
  lastIso: string | null | undefined,
): number | null {
  if (!firstIso || !lastIso) return null;
  const a = Date.parse(firstIso);
  const b = Date.parse(lastIso);
  if (Number.isNaN(a) || Number.isNaN(b)) return null;
  return Math.max(0, b - a);
}

/** Convenience: `formatDurationSpan(spanMs(first, last))`. */
export function formatSpan(
  firstIso: string | null | undefined,
  lastIso: string | null | undefined,
): string {
  return formatDurationSpan(spanMs(firstIso, lastIso));
}

export function formatUsd(usd: number | null | undefined): string {
  if (usd === null || usd === undefined || Number.isNaN(usd)) return PLACEHOLDER;
  if (usd < 0) return PLACEHOLDER;
  if (usd < 1) return `$${usd.toFixed(4)}`;
  return `$${usd.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function formatTokens(n: number | null | undefined): string {
  if (bad(n)) return PLACEHOLDER;
  const v = n as number;
  if (v < 1000) return `${Math.round(v)}`;
  if (v < 1_000_000) return `${(v / 1000).toFixed(1)}k`;
  return `${(v / 1_000_000).toFixed(1)}M`;
}

export function formatPct(ratio: number | null | undefined): string {
  if (bad(ratio)) return PLACEHOLDER;
  const v = ratio as number;
  if (v > 1) return PLACEHOLDER;
  return `${Math.round(v * 100)}%`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (bad(bytes)) return PLACEHOLDER;
  const v = bytes as number;
  if (v < 1024) return `${Math.round(v)} B`;
  if (v < 1_048_576) return `${(v / 1024).toFixed(1)} KiB`;
  if (v < 1_073_741_824) return `${(v / 1_048_576).toFixed(1)} MiB`;
  return `${(v / 1_073_741_824).toFixed(1)} GiB`;
}

/**
 * S6 (UX 재설계) — relative time for the session list. "just now" / "방금"
 * under a minute, then minutes / hours / days, falling back to a `YYYY-MM-DD`
 * date past 30 days so old rows read as a date, not "812 days ago". Returns "—"
 * for unparseable input. `nowMs` is injected so the formatter stays pure and
 * testable. l10n — the wording is keyed off `locale` (English by default, the
 * source language); the unit words live here rather than in the catalog to keep
 * format.ts a pure function with no i18n-Provider coupling. The caller injects
 * the active locale via useLocale().
 */
export function relativeTime(
  iso: string | null | undefined,
  nowMs: number,
  locale: Locale = 'en',
): string {
  if (!iso) return PLACEHOLDER;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return PLACEHOLDER;
  const ko = locale === 'ko';
  const diffMs = nowMs - t;
  const sec = Math.floor(Math.max(diffMs, 0) / 1000);
  if (sec < 60) return ko ? '방금' : 'just now';
  const min = Math.floor(sec / 60);
  if (min < 60) return ko ? `${min}분 전` : `${min} min ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return ko ? `${hr}시간 전` : `${hr} hr ago`;
  const day = Math.floor(hr / 24);
  if (day <= 30) return ko ? `${day}일 전` : `${day} day${day === 1 ? '' : 's'} ago`;
  return new Date(t).toISOString().slice(0, 10);
}

/**
 * S6 — humanise a raw Claude model id (`claude-opus-4-8` → "Opus 4.8"). Drops
 * a trailing date stamp (`-20251001`) and a `[1m]` context suffix. Falls back
 * to the raw id when the shape is unknown, "—" for empty/nullish.
 */
export function formatModel(id: string | null | undefined): string {
  if (!id) return PLACEHOLDER;
  const cleaned = id.replace(/\[[^\]]*\]$/, '');
  // 표시명 규칙의 SSOT는 seriesView.displayModel — minor 없는 이름
  // ('claude-fable-5')과 날짜 접미까지 한 곳에서 처리한다(2026-07-04 통일).
  return displayModel(cleaned);
}
