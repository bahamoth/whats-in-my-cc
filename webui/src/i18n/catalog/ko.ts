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
  'lang.group': '언어',
  'lang.switchToEnglish': 'Switch to English',
  'lang.switchToKorean': '한국어로 전환',

  // Common
  'common.loadingEarlier': '이전 메시지 불러오는 중…',

  // Session list
  'sessions.searchPlaceholder': '⌕ 프로젝트·슬러그 검색… ( / )',
  'sessions.searchAria': '세션 검색',

  // Session detail
  'detail.analysisToggle': '분석',

  // Insight strip
  'insight.stripAria': '세션 인사이트',
  'insight.infoTipAria': '{label} 설명',
  'insight.recollectUsage': 'usage facet 재수집 필요',
  'insight.loading': '로딩 중',
  'insight.baselineDeltaPp': (sd: string) => `${sd}%p vs 중앙값`,
  'insight.baselineDeltaPct': (sd: string) => `${sd}% vs 중앙값`,
  'insight.provenance.measured': '측정',
  'insight.provenance.mixed': '혼합',
  'insight.provenance.estimated': '추정',
  'insight.provenance.uncollected': '미수집·예정',

  'insight.context.title': '컨텍스트 효율',
  'insight.context.tip':
    '캐시 적중률 = cache_read / (cache_read + cache_creation + input). 측정값(usage facet). ' +
    '고정 캐시 컨텍스트 크기·증가·캐시 미스는 펼쳐서 확인. 시스템 프롬프트/스킬/메모리 단위 분해와 ' +
    '"오염" 판정은 데이터에 없어 제공하지 않습니다(설계 §8 한계).',
  'insight.context.detailCacheRead': (v: string) => `캐시 읽기 ${v}`,
  'insight.context.drillHitRate': (v: string) => `캐시 적중률 ${v}`,
  'insight.context.drillCacheReadFree': (v: string) => `캐시 읽기(무료) ${v}`,
  'insight.context.drillCacheCreation': (v: string) => `캐시 생성 ${v}`,
  'insight.context.drillUserTurns': (n: number) => `사용자 턴 ${n}`,

  'insight.tokens.title': '토큰',
  'insight.tokens.tip':
    '청구 토큰(input + cache_creation + output)과 캐시 읽기(무료)는 의미가 달라 절대 합산하지 않습니다 ' +
    '(설계 §3 Q2). 측정값(usage facet).',
  'insight.tokens.valueBilled': (v: string) => `청구 ${v}`,
  'insight.tokens.detailCacheReadFree': (v: string) => `캐시 읽기 ${v} (무료)`,
  'insight.tokens.drillByModel': (a: { model: string; events: number; out: string }) =>
    `${a.model}: ${a.events} 산출 · 출력 ${a.out}`,

  'insight.verification.title': '검증',
  'insight.verification.tip':
    '가드 = 실행된 테스트/빌드/린트/포맷 검사. 알려진 도구 매칭(known_tool) + 종료코드(exit) 기반이면 측정, ' +
    '파이프(piped)로 가려진 종료코드가 섞이면 혼합으로 표시(슬라이스 2 detection_basis/status_basis). ' +
    '키워드 추정(test_keyword)은 더 이상 생성되지 않으며(F2), 과거 ingest된 older 데이터에만 나타날 수 있습니다. ' +
    '브라우저 스모크/서브에이전트 테스트는 감지하지 않습니다(설계 §3 Q4 한계).',
  'insight.verification.noGuards': '감지된 가드 없음',
  'insight.verification.valuePassed': (a: { total: number; passed: number; measured: number }) =>
    `가드 ${a.total} · 통과 ${a.passed}/${a.measured}`,
  'insight.verification.valueNoMeasure': (total: number) => `가드 ${total} · 측정 없음`,
  'insight.verification.unmeasured': (n: number) => `미측정 ${n}`,
  'insight.verification.estimatedSuffix': ' (추정)',

  'insight.toolFailure.title': '도구 실패',
  'insight.toolFailure.tip':
    '도구 실패 signal 수(detector=tool_failure). 결정적 카운트이며 심각도 판단은 포함하지 않습니다.',
  'insight.toolFailure.none': '도구 실패 없음',
  'insight.toolFailure.expand': '펼쳐서 확인',

  'insight.cost.title': '비용',
  'insight.cost.tip':
    '공개 가격표 × usage 토큰으로 계산한 추정치이며 실제 청구액이 아닙니다(설계 §6.5/§11.3). ' +
    'OTel claude_code.cost.usage 메트릭이 들어오면 대체됩니다. cache_read(무료)는 비용에서 제외.',
  'insight.cost.detailEstimate': '공개 가격표 추정 (≈)',
  'insight.cost.detailEstimateUnpriced': (n: number) => `공개 가격표 추정 (≈) · 미가격 ${n}`,
  'insight.cost.noPricing': '가격표 없음',
};
