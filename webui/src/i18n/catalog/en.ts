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

  // Insight strip
  'insight.stripAria': 'Session insights',
  'insight.infoTipAria': '{label} explanation',
  'insight.recollectUsage': 'usage facet re-collection needed',
  'insight.loading': 'Loading',
  'insight.baselineDeltaPp': (sd: string) => `${sd}%p vs median`,
  'insight.baselineDeltaPct': (sd: string) => `${sd}% vs median`,
  'insight.provenance.measured': 'measured',
  'insight.provenance.mixed': 'mixed',
  'insight.provenance.estimated': 'estimated',
  'insight.provenance.uncollected': 'uncollected·planned',

  'insight.context.title': 'Context efficiency',
  'insight.context.tip':
    'Cache hit ratio = cache_read / (cache_read + cache_creation + input). Measured (usage facet). ' +
    'Expand for the fixed cached-context size, its growth, and cache misses. Per system-prompt/skill/memory ' +
    'breakdown and a "contamination" verdict are not in the data, so not provided (design §8 limitation).',
  'insight.context.detailCacheRead': (v: string) => `Cache reads ${v}`,
  'insight.context.drillHitRate': (v: string) => `Cache hit rate ${v}`,
  'insight.context.drillCacheReadFree': (v: string) => `Cache reads (free) ${v}`,
  'insight.context.drillCacheCreation': (v: string) => `Cache creation ${v}`,
  'insight.context.drillUserTurns': (n: number) => `User turns ${n}`,

  'insight.tokens.title': 'Tokens',
  'insight.tokens.tip':
    'Billed tokens (input + cache_creation + output) and cache reads (free) mean different things and are ' +
    'never summed (design §3 Q2). Measured (usage facet).',
  'insight.tokens.valueBilled': (v: string) => `Billed ${v}`,
  'insight.tokens.detailCacheReadFree': (v: string) => `Cache reads ${v} (free)`,
  'insight.tokens.drillByModel': (a: { model: string; events: number; out: string }) =>
    `${a.model}: ${a.events} produced · output ${a.out}`,

  'insight.verification.title': 'Verification',
  'insight.verification.tip':
    'Guards = the test/build/lint/format checks that ran. Measured when based on a known-tool match (known_tool) ' +
    'plus an exit code (exit); shown as mixed when piped-masked exit codes are mixed in (slice 2 ' +
    'detection_basis/status_basis). Keyword guesses (test_keyword) are no longer produced (F2) and only appear in ' +
    'older ingested data. Browser smoke / subagent tests are not detected (design §3 Q4 limitation).',
  'insight.verification.noGuards': 'No guards detected',
  'insight.verification.valuePassed': (a: { total: number; passed: number; measured: number }) =>
    `${a.total} guards · ${a.passed}/${a.measured} passed`,
  'insight.verification.valueNoMeasure': (total: number) => `${total} guards · not measured`,
  'insight.verification.unmeasured': (n: number) => `${n} unmeasured`,
  'insight.verification.estimatedSuffix': ' (estimated)',

  'insight.toolFailure.title': 'Tool failures',
  'insight.toolFailure.tip':
    'Tool-failure signal count (detector=tool_failure). A deterministic count; it does not judge severity.',
  'insight.toolFailure.none': 'No tool failures',
  'insight.toolFailure.expand': 'Expand to view',

  'insight.cost.title': 'Cost',
  'insight.cost.tip':
    'An estimate computed from the public price list × usage tokens — not the actual bill (design §6.5/§11.3). ' +
    'Replaced once the OTel claude_code.cost.usage metric arrives. cache_read (free) is excluded from cost.',
  'insight.cost.detailEstimate': 'Public-pricing estimate (≈)',
  'insight.cost.detailEstimateUnpriced': (n: number) => `Public-pricing estimate (≈) · ${n} unpriced`,
  'insight.cost.noPricing': 'no pricing',
};

export type Messages = typeof en;
export type MessageKey = keyof Messages;
