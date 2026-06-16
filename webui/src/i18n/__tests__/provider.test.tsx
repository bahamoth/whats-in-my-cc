// l10n — the React surface: I18nProvider seeds the locale, useT exposes the
// bound translator, useLocale exposes the current locale + a persisting setter.
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nProvider, useT, useLocale } from '../index';
import { STORAGE_KEY } from '../detect';

function Probe() {
  const t = useT();
  const { locale, setLocale } = useLocale();
  return (
    <div>
      <span data-testid="msg">{t('nav.sessions')}</span>
      <span data-testid="loc">{locale}</span>
      <button onClick={() => setLocale('en')}>to-en</button>
      <button onClick={() => setLocale('ko')}>to-ko</button>
    </div>
  );
}

describe('I18nProvider', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('renders messages in the initial locale', () => {
    render(
      <I18nProvider initialLocale="ko">
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId('loc').textContent).toBe('ko');
    expect(screen.getByTestId('msg').textContent).toBe('세션');
  });

  it('switches language, persists the choice, and updates <html lang>', async () => {
    const user = userEvent.setup();
    render(
      <I18nProvider initialLocale="ko">
        <Probe />
      </I18nProvider>,
    );
    await user.click(screen.getByText('to-en'));
    expect(screen.getByTestId('loc').textContent).toBe('en');
    expect(screen.getByTestId('msg').textContent).toBe('Sessions');
    expect(localStorage.getItem(STORAGE_KEY)).toBe('en');
    expect(document.documentElement.lang).toBe('en');
  });

  it('throws when useT is used outside a provider', () => {
    function Lonely() {
      useT();
      return null;
    }
    // Silence React's console.error noise for the expected throw.
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Lonely />)).toThrow();
    spy.mockRestore();
  });
});
