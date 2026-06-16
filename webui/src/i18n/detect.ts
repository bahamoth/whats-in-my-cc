// l10n — supported locales and locale resolution. English is the source /
// fallback language; Korean is the only other supported locale. Resolution
// precedence: an explicit stored choice, then the browser's preferred
// language, then the English default.

export const LOCALES = ['en', 'ko'] as const;
export type Locale = (typeof LOCALES)[number];
export const DEFAULT_LOCALE: Locale = 'en';

/** localStorage key holding the user's manual choice. */
export const STORAGE_KEY = 'wimcc.lang';

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (LOCALES as readonly string[]).includes(value);
}

/**
 * Pure precedence resolver. `stored` is the persisted manual choice (or null);
 * `navLang` is `navigator.language` (or null). Kept pure so its precedence is
 * testable without touching globals.
 */
export function resolveLocale(stored: string | null, navLang: string | null): Locale {
  if (isLocale(stored)) return stored;
  if (navLang && navLang.toLowerCase().startsWith('ko')) return 'ko';
  return DEFAULT_LOCALE;
}

/** Read the persisted choice; null when unset or storage is unavailable. */
export function readStoredLocale(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

/** Resolve the locale from the live environment (localStorage + navigator). */
export function detectLocale(): Locale {
  const navLang = typeof navigator !== 'undefined' ? navigator.language : null;
  return resolveLocale(readStoredLocale(), navLang);
}
