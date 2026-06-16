// l10n — the navRail language switcher. Two buttons (EN / KO); the active one
// is aria-pressed, clicking the other switches the app locale. Accessible
// labels are themselves localized.
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { I18nProvider, type Locale } from '../../../i18n';
import { LanguageToggle } from '../LanguageToggle';

function setup(initial: Locale) {
  return render(
    <I18nProvider initialLocale={initial}>
      <LanguageToggle />
    </I18nProvider>,
  );
}

describe('LanguageToggle', () => {
  beforeEach(() => localStorage.clear());

  it('marks the active locale button as pressed', () => {
    setup('ko');
    expect(screen.getByTestId('lang-toggle-ko')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('lang-toggle-en')).toHaveAttribute('aria-pressed', 'false');
  });

  it('switches the active locale on click', async () => {
    const user = userEvent.setup();
    setup('ko');
    await user.click(screen.getByTestId('lang-toggle-en'));
    expect(screen.getByTestId('lang-toggle-en')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('lang-toggle-ko')).toHaveAttribute('aria-pressed', 'false');
  });

  it('gives each button a localized accessible label', () => {
    setup('en');
    expect(screen.getByTestId('lang-toggle-en')).toHaveAttribute(
      'aria-label',
      'Switch to English',
    );
    expect(screen.getByTestId('lang-toggle-ko')).toHaveAttribute(
      'aria-label',
      'Switch to Korean',
    );
  });
});
