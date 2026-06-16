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

  // Analysis panel
  'analysis.empty': '분석할 지표가 없습니다.',
  'analysis.sessionMetrics': '세션 지표',
  'analysis.toolFailures': '도구 실패',
  'analysis.verificationPassed': '검증 통과 (측정분)',
  'analysis.unmeasuredInline': (n: number) => ` · 미측정 ${n}`,
  'analysis.noMeasurement': '측정 없음',
  'analysis.contextBloatCount': 'Context bloat 횟수',
  'analysis.detectorDistribution': 'Detector 신호 분포',
  'analysis.noSignals': '감지된 신호 없음',
  'analysis.reReadLabel': (a: { file: string; count: string }) => `${a.file} · ${a.count}회`,

  // Detail — event provenance badge
  'detail.provenance.native': '원본',
  'detail.provenance.derived': '가공',

  // Detail — InsightTab
  'detail.insightTab.nativeTitle': 'Claude Code 원본 관측',
  'detail.insightTab.derivedTitle': 'wimcc 파생 데이터',
  'detail.insightTab.copyTitle': (a: { field: string; value: string }) =>
    `${a.field}: ${a.value} — 클릭하여 복사`,
  'detail.insightTab.jumpToEvidence': '증거 이벤트로 이동',
  'detail.insightTab.evidenceHint': '↳ 증거',
  'detail.insightTab.whatTitle': 'What — 한 일',
  'detail.insightTab.howTitle': 'How — 지표',

  // Detail — WhatSection
  'detail.what.errorBadge': '오류',
  'detail.what.truncNote': (chars: string) => `… ${chars}자 이후 잘림 — Raw 탭에서 전문`,
  'detail.what.thinkingNotRecorded': '추론 본문은 기록되지 않음 (signature only)',
  'detail.what.rawFallback': '원본은 Raw 탭 참조',

  // Detail — ResponseMetricsPanel
  'detail.response.title': '추론 · 응답 지표',
  'detail.response.note':
    '추론 내용은 transcript에 기록되지 않습니다(암호화된 signature만 존재). ' +
    '아래는 이 응답(LLM request)의 실측 지표입니다.',
  'detail.response.empty': '이 응답의 지표를 현재 윈도우에서 찾지 못했습니다.',
  'detail.response.warn': '이 응답은 잘림/재시도/실패 신호가 있습니다.',

  // Metrics rows (shared by EntityMetricsPanel + ResponseMetricsPanel)
  'metric.uncollected': '지표 미수집',
  'metric.attemptCount': (n: number) => `${n}회`,
  'metric.yes': '예',
  'metric.no': '아니오',
  'metric.group.toolExec': '도구 실행',
  'metric.group.hookExec': 'hook 실행',
  'metric.group.llmActivity': 'LLM 동작',
  'metric.group.tokens': '토큰',
  'metric.group.cost': '비용',
  'metric.label.duration': '소요 시간',
  'metric.label.result': '결과',
  'metric.label.decisionSource': '결정 출처',
  'metric.label.ioSize': '입력/결과 크기',
  'metric.label.sequence': '순서',
  'metric.label.hookEvent': 'hook 이벤트',
  'metric.label.command': '명령',
  'metric.label.ttft': '첫 토큰까지(ttft)',
  'metric.label.stopReason': '종료 사유',
  'metric.label.attempts': '시도',
  'metric.label.success': '성공',
  'metric.label.model': '모델',
  'metric.label.querySource': '요청 출처',
  'metric.label.outputTokens': '출력 토큰',
  'metric.label.outputSpeed': '출력 속도',
  'metric.label.inputTokens': '입력 토큰',
  'metric.label.cacheReads': '캐시 읽기',
  'metric.label.cacheCreation': '캐시 생성',
  'metric.label.billedCost': '청구 비용',
  'metric.tip.outputTokens': '이 응답에서 생성된 토큰 수입니다. 추론(thinking) 토큰도 여기에 포함됩니다 — 모델이 만들어낸 분량.',
  'metric.tip.inputTokens': '이번 요청에 새로 전달된(캐시되지 않은) 입력 토큰 수입니다. 컨텍스트 대부분은 보통 캐시 읽기로 재사용됩니다.',
  'metric.tip.cacheReads': '프롬프트 캐시에서 재사용한 토큰 수입니다. 클수록 컨텍스트 대부분을 캐시로 재활용 — 비용·지연을 줄입니다.',
  'metric.tip.cacheCreation': '이번에 새로 캐시에 기록한 토큰 수입니다. 다음 요청부터 캐시 읽기로 재사용됩니다.',
  'metric.tip.billedCost': '이 요청의 실측 비용(USD)입니다. Claude Code가 보고한 값(api_request_log)으로, 토큰×공개요금 추정과는 다릅니다.',
  'metric.tip.outputSpeed': '생성 처리량입니다 — 출력 토큰 ÷ 요청 소요 시간(초). 추론 토큰도 출력에 포함됩니다.',
  'metric.tip.querySource': '이 요청을 보낸 주체입니다. 메인 스레드(사용자 대화) 또는 서브에이전트(general-purpose·Explore 등) — 누가 호출했는지.',
  'metric.tip.decisionSource': '이 도구 실행이 허용된 경위입니다. config = 설정에 의해 자동 허용, user = 사용자가 직접 승인 등 — 권한 결정의 출처.',
  'metric.tip.ioSize': '도구에 전달한 입력과 도구가 반환한 결과의 크기(바이트)입니다. 결과가 클수록 컨텍스트를 많이 차지합니다.',
};
