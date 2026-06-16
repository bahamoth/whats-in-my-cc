// l10n — English catalog. This is the SOURCE OF TRUTH: `typeof en` defines the
// `Messages` type, so every other locale (ko.ts) must provide exactly these
// keys with compatible value types or it fails to compile (missing key, typo,
// or string-vs-function mismatch all become type errors). Keys are flat,
// dotted, grouped by area. Function values handle counts / plurals.

export const en = {
  // App shell / navigation
  'nav.sessions': 'Sessions',
  'a11y.skipToContent': 'Skip to content',
  'a11y.primaryNav': 'Primary',
  'a11y.sidePanel': 'Side panel',

  // Language switcher
  'lang.group': 'Language',
  'lang.switchToEnglish': 'Switch to English',
  'lang.switchToKorean': 'Switch to Korean',

  // Common
  'common.loadingEarlier': 'Loading earlier messages…',

  // Session list
  'sessions.searchPlaceholder': '⌕ Search projects · slugs… ( / )',
  'sessions.searchAria': 'Search sessions',

  // Session detail
  'detail.analysisToggle': 'Analysis',
};

export type Messages = typeof en;
export type MessageKey = keyof Messages;
