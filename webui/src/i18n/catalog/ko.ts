// l10n — Korean catalog. Typed as `Messages` so its key set must match en.ts
// exactly; the compiler rejects a missing, extra, or wrongly-shaped entry.
import type { Messages } from './en';

export const ko: Messages = {
  // App shell / navigation
  'nav.sessions': '세션',
  'a11y.skipToContent': '본문으로 건너뛰기',
  'a11y.primaryNav': '주 메뉴',
  'a11y.sidePanel': '사이드 패널',

  // Language switcher
  'lang.switchToEnglish': 'Switch to English',
  'lang.switchToKorean': '한국어로 전환',
};
