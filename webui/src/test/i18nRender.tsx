// l10n test helper — components that call useT()/useLocale() must render inside
// an <I18nProvider>. Most component tests assert the Korean source strings, so
// this defaults to the `ko` locale; pass a locale to override. Using RTL's
// `wrapper` option means the returned `rerender` keeps the provider too.
import type { ReactElement } from 'react';
import { render as rtlRender, type RenderOptions } from '@testing-library/react';
import { I18nProvider, type Locale } from '../i18n';

export function renderWithI18n(
  ui: ReactElement,
  locale: Locale = 'ko',
  options?: Omit<RenderOptions, 'wrapper'>,
) {
  return rtlRender(ui, {
    wrapper: ({ children }) => (
      <I18nProvider initialLocale={locale}>{children}</I18nProvider>
    ),
    ...options,
  });
}
