// l10n — `translate` is the lookup + interpolation core. One rule:
//  - function message  → call it with the arg (counts / plurals live here),
//  - string + object   → replace {name} placeholders,
//  - string + nothing  → return as-is,
//  - missing in locale  → fall back to the English catalog,
//  - missing in both    → return the key (defensive; type system prevents it).
import { describe, expect, it, vi } from 'vitest';
import { translate, type Catalog } from '../t';

const en: Catalog = {
  'nav.sessions': 'Sessions',
  'wf.concurrent': (n) => `${n} concurrent`,
  'detail.greeting': 'Hi {name}, {count} events',
  'only.en': 'English only',
};

const ko: Catalog = {
  'nav.sessions': '세션',
  'wf.concurrent': (n) => `${n}건 동시`,
  'detail.greeting': '{name}님, {count}건',
  // 'only.en' intentionally absent to exercise fallback
};

describe('translate', () => {
  it('returns a plain string message as-is', () => {
    expect(translate(ko, en, 'nav.sessions')).toBe('세션');
    expect(translate(en, en, 'nav.sessions')).toBe('Sessions');
  });

  it('calls a function message with the arg', () => {
    expect(translate(en, en, 'wf.concurrent', 3)).toBe('3 concurrent');
    expect(translate(ko, en, 'wf.concurrent', 3)).toBe('3건 동시');
  });

  it('interpolates {name} placeholders from an object arg', () => {
    expect(translate(en, en, 'detail.greeting', { name: 'Ada', count: 5 })).toBe(
      'Hi Ada, 5 events',
    );
    expect(translate(ko, en, 'detail.greeting', { name: 'Ada', count: 5 })).toBe(
      'Ada님, 5건',
    );
  });

  it('leaves unknown placeholders untouched', () => {
    expect(translate(en, en, 'detail.greeting', { name: 'Ada' })).toBe(
      'Hi Ada, {count} events',
    );
  });

  it('falls back to the English catalog when the locale lacks the key', () => {
    expect(translate(ko, en, 'only.en')).toBe('English only');
  });

  it('returns the key when it is missing from both catalogs', () => {
    const spy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(translate(ko, en, 'does.not.exist')).toBe('does.not.exist');
    spy.mockRestore();
  });
});
