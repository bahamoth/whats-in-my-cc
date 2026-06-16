// l10n — public API. Wrap the app in <I18nProvider>, then call useT() for the
// bound translator and useLocale() for the current locale + a persisting
// setter. English is the fallback catalog for any key missing in the active
// locale (defensive — the Messages type already enforces parity).
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { en, type MessageKey } from './catalog/en';
import { ko } from './catalog/ko';
import { STORAGE_KEY, detectLocale, type Locale } from './detect';
import { translate, type Catalog, type MessageArg } from './t';

export { LOCALES, DEFAULT_LOCALE, type Locale } from './detect';
export type { MessageKey } from './catalog/en';

const catalogs: Record<Locale, Catalog> = { en, ko };

export type TFunction = (key: MessageKey, arg?: MessageArg) => string;

interface I18nValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: TFunction;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({
  children,
  initialLocale,
}: {
  children: ReactNode;
  initialLocale?: Locale;
}) {
  const [locale, setLocaleState] = useState<Locale>(
    () => initialLocale ?? detectLocale(),
  );

  useEffect(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = locale;
    }
  }, [locale]);

  const setLocale = useCallback((next: Locale) => {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // storage unavailable (private mode / quota) — keep working in-memory
    }
    setLocaleState(next);
  }, []);

  const t = useCallback<TFunction>(
    (key, arg) => translate(catalogs[locale] ?? en, en, key, arg),
    [locale],
  );

  const value = useMemo<I18nValue>(
    () => ({ locale, setLocale, t }),
    [locale, setLocale, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error('useT / useLocale must be used within an <I18nProvider>');
  }
  return ctx;
}

export function useT(): TFunction {
  return useI18n().t;
}

export function useLocale(): { locale: Locale; setLocale: (locale: Locale) => void } {
  const { locale, setLocale } = useI18n();
  return { locale, setLocale };
}
