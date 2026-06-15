/**
 * PR-3 — display formatters. Always return "—" for null / undefined /
 * NaN / negative values so the UI never shows raw `NaN` or `undefined`.
 */

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
 * S6 (UX 재설계) — Korean relative time for the session list. "방금" under a
 * minute, then `N분/시간/일 전`, falling back to a `YYYY-MM-DD` date past 30
 * days so old rows read as a date, not "812일 전". Returns "—" for unparseable
 * input. `nowMs` is injected so the formatter stays pure and testable.
 */
export function relativeTime(iso: string | null | undefined, nowMs: number): string {
  if (!iso) return PLACEHOLDER;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return PLACEHOLDER;
  const diffMs = nowMs - t;
  if (diffMs < 0) return '방금';
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return '방금';
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}분 전`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}시간 전`;
  const day = Math.floor(hr / 24);
  if (day <= 30) return `${day}일 전`;
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
  const m = cleaned.match(/^claude-(opus|sonnet|haiku|fable)-(\d+)-(\d+)/);
  if (!m) return id;
  const family = m[1].charAt(0).toUpperCase() + m[1].slice(1);
  return `${family} ${m[2]}.${m[3]}`;
}
