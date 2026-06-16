// l10n — locale resolution is a pure function so we can lock its precedence
// without mocking globals: stored choice wins, else navigator.language prefix,
// else the English default.
import { describe, expect, it } from 'vitest';
import { resolveLocale, isLocale, DEFAULT_LOCALE } from '../detect';

describe('resolveLocale', () => {
  it('prefers a valid stored choice over navigator language', () => {
    expect(resolveLocale('en', 'ko-KR')).toBe('en');
    expect(resolveLocale('ko', 'en-US')).toBe('ko');
  });

  it('ignores an invalid stored value and falls through', () => {
    expect(resolveLocale('fr', 'ko-KR')).toBe('ko');
    expect(resolveLocale('', 'en-US')).toBe('en');
    expect(resolveLocale(null, 'ko')).toBe('ko');
  });

  it('maps a Korean navigator language to ko', () => {
    expect(resolveLocale(null, 'ko')).toBe('ko');
    expect(resolveLocale(null, 'ko-KR')).toBe('ko');
    expect(resolveLocale(null, 'KO')).toBe('ko');
  });

  it('maps any non-Korean navigator language to the English default', () => {
    expect(resolveLocale(null, 'en-US')).toBe('en');
    expect(resolveLocale(null, 'fr-FR')).toBe('en');
    expect(resolveLocale(null, 'ja')).toBe('en');
  });

  it('falls back to the default when nothing is known', () => {
    expect(resolveLocale(null, null)).toBe(DEFAULT_LOCALE);
    expect(DEFAULT_LOCALE).toBe('en');
  });
});

describe('isLocale', () => {
  it('accepts supported locales only', () => {
    expect(isLocale('en')).toBe(true);
    expect(isLocale('ko')).toBe(true);
    expect(isLocale('fr')).toBe(false);
    expect(isLocale(null)).toBe(false);
    expect(isLocale(undefined)).toBe(false);
  });
});
