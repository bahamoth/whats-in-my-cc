// l10n — compact EN/KO switcher pinned to the bottom of the navRail. The
// active locale is aria-pressed; each button carries a localized accessible
// label. Visible glyphs ("EN"/"KO") stay language-neutral codes.
import { useLocale, useT, type Locale } from '../../i18n';
import styles from './LanguageToggle.module.css';

const OPTIONS: ReadonlyArray<{ locale: Locale; code: string; labelKey: 'lang.switchToEnglish' | 'lang.switchToKorean' }> = [
  { locale: 'en', code: 'EN', labelKey: 'lang.switchToEnglish' },
  { locale: 'ko', code: 'KO', labelKey: 'lang.switchToKorean' },
];

export function LanguageToggle() {
  const { locale, setLocale } = useLocale();
  const t = useT();

  return (
    <div className={styles.toggle} role="group" aria-label={t('lang.group')}>
      {OPTIONS.map(({ locale: opt, code, labelKey }) => {
        const active = locale === opt;
        return (
          <button
            key={opt}
            type="button"
            data-testid={`lang-toggle-${opt}`}
            className={active ? `${styles.btn} ${styles.active}` : styles.btn}
            aria-pressed={active}
            aria-label={t(labelKey)}
            onClick={() => setLocale(opt)}
          >
            {code}
          </button>
        );
      })}
    </div>
  );
}
