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
  'detail.taskBoard.title': 'Tasks',
  'detail.taskBoard.done': '완료',
  'detail.taskBoard.noInProgress': 'in_progress 미관측',
  'detail.plugin.title': 'Plugin',
  'detail.plugin.configured': '직접 설정한 MCP 서버 (마켓플레이스 plugin 아님).',
  'detail.plugin.connector': 'Anthropic 공식 통합 (관리되는 커넥터 / 확장).',

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

  // Stream — shared
  'stream.reasoning': '추론',
  'stream.notification': '알림',
  'stream.output': '출력',
  'stream.conclusion': '결론',
  'stream.synthesisLabel': '종합',
  'stream.inProgress': '진행 중',
  'stream.concurrentMain': (n: number) => `⟂ main ${n}건 동시`,
  'stream.laneExpandTitle': (label: string) => `${label} — 클릭하여 펼치기`,
  'stream.conversationStart': '대화 시작',
  'stream.workflow.agentN': (n: number) => `에이전트 ${n}`,

  // Stream — autoscroll
  'stream.autoscroll.label': '자동 스크롤',
  'stream.autoscroll.disableAria': '자동 스크롤 끄기',
  'stream.autoscroll.enableAria': '자동 스크롤 켜고 최신으로 이동',

  // Stream — scaffold group
  'stream.scaffold.chip': '커맨드·스킬',
  'stream.scaffold.sourcePlus': (n: number) => `+출처 ${n}`,

  // Stream — thinking marker
  'stream.thinking.aria': '추론 — 클릭하면 응답 지표 표시 (내용은 transcript에 미수록)',
  'stream.thinking.title': '추론 내용은 transcript에 기록되지 않습니다 (signature만 존재). 클릭하면 응답 지표를 봅니다.',
  'stream.thinking.warnAria': '이상 응답',

  // Stream — end cards
  'stream.endCard.label': '종료',
  'stream.endCard.jumpToNotification': '종료 알림 원문으로 이동',
  'stream.workflowEndCard.label': '워크플로우 종료',
  'stream.workflowEndCard.result': '결과',

  // Stream — untagged Bash panel
  'stream.untagged.jumpTitle': '이 명령의 카드로 이동',
  'stream.untagged.jumpLabel': '카드로 ↗',

  // Stream — legend
  'stream.lane.user': '사용자',
  'stream.lane.scaffold': '스캐폴드',
  'stream.lane.tool': '도구',
  'stream.lane.thinking': '추론',
  'stream.lane.batch': '배치',
  'stream.lane.workflow': '워크플로우',
  'stream.legend.aria': '스트림 범례',
  'stream.legend.move': '이동',
  'stream.legend.nextError': '다음 오류',
  'stream.legend.close': '범례 닫기',
  'stream.legend.show': '범례',

  // Stream — message card
  'stream.msg.summary': '요약',
  'stream.msg.viewRaw': '원본 보기',
  'stream.msg.viewMarkdown': '마크다운 보기',
  'stream.msg.bgTitle': (n: number) =>
    `이 메시지가 진행되는 동안 백그라운드 서브에이전트 ${n}개가 실행 중이었음 (이 메시지가 백그라운드라는 뜻이 아님)`,
  'stream.msg.bgRunning': (n: number) => `⟂ 백그라운드 ${n}개 실행 중`,
  'stream.msg.done': '완료',
  'stream.msg.jumpToCommand': '원래 명령으로 이동',
  'stream.msg.commandJump': '↳ 명령',
  'stream.msg.openSession': '세션 열기 →',
  'stream.msg.openTeammateSession': '이 팀메이트 세션의 replay로 이동',
  'team.agentType': '에이전트 타입',
  'team.lead': '리드 세션',
  'team.teammates': (n: number) => `팀메이트 ${n}개`,
  'stream.msg.jumpToDispatch': '이 팀메이트를 띄운 Agent 디스패치로 이동',
  'stream.msg.dispatch': '디스패치',
  'stream.msg.showMore': '더 보기',
  'stream.msg.collapse': '접기',

  // Stream — batch group
  'stream.batch.chip': '병렬 배치',

  // Stream — subagent group
  'stream.subagent.messages': (n: number) => `메시지 ${n}`,
  'stream.subagent.tools': (n: number) => `도구 ${n}`,
  'stream.subagent.done': '✓ 완료',
  'stream.subagent.running': '● 실행 중',
  'stream.subagent.jumpToTask': '호출한 Task로 이동',

  // Stream — workflow group
  'stream.workflow.chip': '워크플로우',
  'stream.workflow.jumpTitle': '이 워크플로우를 띄운 Workflow 호출로 이동',
  'stream.workflow.jumpAria': 'Workflow 호출로 이동',
  'stream.workflow.call': '호출',
  'stream.workflow.maxConcurrency': '최대 병렬',
  'stream.workflow.longest': '최장',
  'stream.workflow.median': '중앙값',
  'stream.workflow.incomplete': '미완',

  // Stream — query source (who issued the LLM request)
  'stream.querySource.mainThread': '메인 스레드',
  'stream.querySource.subagent': (name: string) => `서브에이전트 · ${name}`,

  // Project dashboard (B-1)
  'nav.dashboard': '대시보드',
  'dash.tab.overview': '개요',
  'dash.tab.verification': '검증',
  'dash.head.pass': '검증 통과',
  'dash.head.cost': '추정 비용',
  'dash.head.rate': '블렌디드 단가',
  'dash.head.hit': '캐시 적중',
  'dash.head.toolfail': '도구 실패율',
  'dash.head.noCompare': '비교 없음(전체 창)',
  'dash.head.costBasis': '공개 가격표 ≈ · 하한',
  'dash.head.ratePer': '과금 토큰 1M당',
  'dash.head.hitBasis': '입력 컨텍스트 합계 기준',
  'dash.head.toolfailOf': (a: { fails: number; calls: string }) => `${a.fails} / ${a.calls} 호출`,
  'dash.head.guards': (n: number) => `창 내 가드 ${n}`,
  'dash.observed': '관측된 변화',
  'dash.observed.modelFirst': (a: { date: string; model: string }) => `${a.date} ${a.model} 첫 관측`,
  'dash.observed.ccSpan': (a: { from: string; to: string; count: number }) =>
    a.count > 1 ? `Claude Code ${a.from} → ${a.to} · 전환 ${a.count}회` : `Claude Code ${a.from} → ${a.to}`,
  'dash.observed.topSignals': (a: { name: string; n: number }) => `신호 최다 ${a.name} (${a.n})`,
  'dash.ver.error': '검증 요약을 불러오지 못했습니다.',
  'dash.daily.ver.title': '검증 결과 — 일별',
  'dash.daily.ver.zeroGuards': (n: number) => `가드 0 세션 ${n}개`,
  'dash.daily.ver.badge': (a: { n: number; m: number }) => `가드 ${a.n} · 통과 ${a.m}`,
  'dash.daily.cost.title': '일별 비용 · 신호',
  'dash.daily.cost.desc': '막대 높이 = 추정 비용 · 막대 색 = 그날 신호 수',
  'dash.tt.noSessions': '세션 없음',
  'dash.tt.noGuards': '가드 관측 없음',
  'dash.tt.signals': '신호',
  'dash.marker.first': (m: string) => `${m} 첫 관측`,
  'dash.cohort.compareTitle': (label: string) => `코호트 비교 — ${label} 전후`,
  'dash.cohort.introduced': (m: string) => `${m} 유입`,
  'dash.cohort.retired': (m: string) => `${m} 이탈`,
  'dash.cohort.beforeAfter': (a: { b: number; a: number }) => `전 ${a.b} · 후 ${a.a} 세션`,
  'dash.cohort.lowSample': '표본 부족',
  'dash.cohort.ccAlso': '이 경계에서 Claude Code 버전도 함께 변경 — 효과 분리 불가',
  'dash.cohort.sigPerSession': '신호 / 세션',
  'dash.cohort.before': '전',
  'dash.cohort.after': '후',
  'dash.cohort.basis': '측정값만',
  'dash.lane.title': '세션 타임라인',
  'dash.lane.desc': '실제 날짜 위치 · 클릭 시 리플레이',
  'dash.lane.notMeasured': '모델 미관측',
  'dash.lane.sig': (n: number) => `신호 ${n}`,
  'dash.scatter.title': '세션 분포 — 과금 토큰 × 신호 밀도',
  'dash.scatter.desc': 'x 과금 토큰(log) · y 신호/100 events · 크기 비용 · 색 주 모델',
  'dash.scatter.median': '점선 = 중앙값',
  'dash.scatter.x': '과금 토큰(M)',
  'dash.scatter.y': '신호 / 100 events',
  'dash.scatter.click': '클릭 → 리플레이',
  'dash.ver.notExec': '미실행',
  'dash.ver.node.guards': '가드',
  'dash.ver.node.measured': '측정',
  'dash.ver.node.recovered': '복구됨',
  'dash.ver.node.abandoned': '실패 방치',
  'dash.ver.node.piped': '파이프 마스킹',
  'dash.ver.node.other': '기타',
  'dash.ver.head.guards': '가드 실행',
  'dash.ver.head.measured': '측정됨 (exit code)',
  'dash.ver.head.unknownChip': (n: number) => `판정불가 ${n}`,
  'dash.ver.head.unknownSplit': (a: { p: number; o: number }) => `파이프 마스킹 ${a.p} · 기타 ${a.o}`,
  'dash.ver.head.passed': '통과 (측정분)',
  'dash.ver.head.abandoned': '실패 방치',
  'dash.ver.head.abandonedOf': (n: number) => `실패 ${n} 중`,
  'dash.ver.head.abandonedNote': '세션 종료까지 같은 종류의 통과 없음',
  'dash.ver.head.coverage': '변경 커버리지',
  'dash.ver.head.coverageNote': (n: number) => `diff hunk ${n} 중`,
  'dash.ver.kind.title': '종류별 결과 구성',
  'dash.ver.kind.desc': '100% 스택 · 오른쪽 = 건수',
  'dash.ver.flow.title': (n: number) => `가드 ${n}건의 행방`,
  'dash.ver.flow.desc': '측정 → 결과 → 실패의 복구 여부',
  'dash.ver.rhythm.title': '가드 실행 리듬',
  'dash.ver.rhythm.desc': '세션 진행률 위 실행 위치 — 가드 수 상위 세션',
  'dash.ver.rhythm.axis': 'x = 세션 진행률(시간 기준)',
  'dash.ver.rhythm.meta': (a: { g: number; p: number }) => `가드 ${a.g} · 통과 ${a.p}`,
  'dash.ver.cov.title': '변경 커버리지 — 검증 통과가 거친 diff hunk 비율',
  'dash.ver.cov.desc': '커버 / 미커버 diff hunk · hunk 수 상위 세션',
  'dash.ver.cov.overall': (a: { pct: number; n: number }) => `전체 ${a.pct}% · 미커버 ${a.n}`,
  'dash.ver.cov.uncovered': (n: number) => `미커버 ${n}`,
  'dash.range.last30': '최근 30일',
  'dash.range.last90': '최근 90일',
  'dash.range.all': '전체 기간',
  'dash.range.picking': (d: string) => `${d} → 종료일을 고르세요`,
  'dash.range.hint': '캘린더에서 두 날짜를 고르면 사용자 지정 범위가 됩니다.',
  'dash.cohort.secTitle': '코호트 경계',
  'dash.cohort.prefixNote': '전/후 = 창 내 경계 이전/이후 전체 세션',
  'dash.cohort.dim.auto': '자동 — 초과율 상위',
  'dash.cohort.dim.models': '모델',
  'dash.cohort.dim.cc': 'Claude Code',
  'dash.cohort.dim.branch': 'branch',
  'dash.cohort.dim.cwd': 'cwd',
  'dash.cohort.dim.entrypoint': 'entrypoint',
  'dash.cohort.exceed': (x: number) => `임의 분할 대비 상위 ${x}%`,
  'dash.cohort.noneAuto':
    '이 창에서 초과율 게이트를 넘는 경계 없음 — 차원을 선택하면 모든 경계를 탐색할 수 있습니다',
  'dash.cohort.alsoDims': (d: string) => `같은 지점에서 ${d}도 변경 — 효과 분리 불가`,
  'dash.head.pass.tip':
    '측정된 가드 중 통과 비율: 통과 ÷ (통과 + 실패).\n' +
    '가드 = 창 안에서 관측된 테스트 / 빌드 / 린트 / 포맷 실행입니다.\n' +
    '판정불가(exit code가 가려진 실행)는 분모에서 제외합니다.\n' +
    'delta 칩은 직전 동일 길이 창과의 차이입니다.',
  'dash.head.cost.tip':
    '추정 비용의 창 합계: 공개 가격표 × usage 토큰.\n' +
    '청구서가 아니라 하한값입니다 — 구독 할인·서비스 티어는 로컬에서 보이지 않습니다.\n' +
    'cache 읽기는 무료라 제외합니다.',
  'dash.head.rate.tip':
    '블렌디드 단가 = 추정 비용 ÷ 과금 토큰(input + cache 생성 + output) × 1M.\n' +
    '비싼 모델의 비중이 커질수록 올라갑니다.',
  'dash.head.hit.tip':
    '입력 컨텍스트 중 캐시 재사용 비율: cache 읽기 ÷ (input + cache 생성 + cache 읽기).\n' +
    '높을수록 같은 컨텍스트를 싸게 재사용한 것입니다.',
  'dash.head.toolfail.tip':
    '창 안의 도구 호출 중 실패 비율 = 실패 ÷ 전체 호출.\n' +
    '결정론 카운트이며 심각도 판단은 포함하지 않습니다.',
  'dash.observed.tip':
    '모든 항목이 관측에서 자동 파생됩니다:\n' +
    '· 모델 첫 관측 — 이 창에서 그 모델이 처음 나타난 날\n' +
    '· Claude Code — 처음 → 마지막 버전과 전환 횟수\n' +
    '· 신호 최다 — 프로세스 신호 합이 가장 큰 세션',
  'dash.daily.ver.tip':
    '가드 = 관측된 테스트 / 빌드 / 린트 / 포맷 실행.\n' +
    'passed / failed는 exit code로 판정하고, unknown은 exit code가 가려진 실행입니다(파이프 등).\n' +
    '보라 점선은 모델 / Claude Code 전환 지점입니다.',
  'dash.daily.cost.tip':
    '막대 높이 = 그날 세션들의 추정 비용 합(공개 가격표 ≈ · 하한).\n' +
    '막대 색 = 그날 프로세스 신호 수 — 초록(0)에서 붉은색(최대)으로 갈수록 신호가 많던 날입니다.\n' +
    '막대에 hover하면 그날 세션 목록이 보입니다.',
  'dash.cohort.tip':
    '경계 = 세션 환경(fingerprint)이 달라진 지점 — 모델·Claude Code·branch·cwd·entrypoint 5개 차원에서 찾습니다.\n' +
    '"임의 분할 대비 상위 x%" = 이 경계 전후의 지표 차이가, 같은 창을 모든 지점에서 잘라 본 차이들 중 상위 x%라는 뜻입니다. 낮을수록 이 지점의 변화가 두드러집니다.\n' +
    '자동 목록은 상위 10% 이내 경계를 최대 3개 보여줍니다(고정 규칙, 스펙 §2).\n' +
    '전/후 = 창 안에서 경계 이전/이후 전체 세션.',
  'dash.cohort.sig.tip':
    '세션당 프로세스 신호 평균 — 6종(도구 실패·컨텍스트 압축·중단 등)의 합입니다.',
  'dash.lane.tip':
    '카드는 실제 날짜 위치에 놓이고, 같은 날 세션은 줄로 쌓입니다.\n' +
    '카드: 세션 이름 · 모델 · 추정 비용 · 신호 수 · 검증 통과율 · 이벤트 수.\n' +
    '신호 숫자가 앰버면 신호 밀도가 창 중앙값의 2배를 넘은 세션입니다.\n' +
    '점선 테두리의 작은 칩은 무활동 세션(usage·신호·가드 없음)입니다.\n' +
    '카드를 클릭하면 리플레이로 이동합니다.',
  'dash.scatter.tip':
    'x = 과금 토큰(로그 스케일) · y = 이벤트 100건당 신호 수 · 크기 = 추정 비용 · 색 = 주 모델.\n' +
    '점선은 각 축의 중앙값 — 오른쪽 위일수록 많이 쓰고 신호가 잦은 세션입니다.\n' +
    '비용·신호 밀도 상위 세션에는 이름이 붙습니다.\n' +
    '점을 클릭하면 리플레이로 이동합니다.',
  'dash.ver.head.guards.tip':
    '창 안에서 관측된 검증 실행(테스트 / 빌드 / 린트 / 포맷) 총수입니다.',
  'dash.ver.head.measured.tip':
    'exit code가 확보되어 통과 / 실패를 판정할 수 있던 가드입니다.\n' +
    '판정불가의 주 원인은 파이프 마스킹 — `cmd | tail`처럼 파이프를 거치면 exit code가 가려집니다.',
  'dash.ver.head.passed.tip': '측정된 가드(통과 + 실패) 중 통과 비율입니다.',
  'dash.ver.head.abandoned.tip':
    '실패한 가드 중, 세션 종료까지 같은 종류의 통과가 다시 관측되지 않은 것입니다.\n' +
    '빨강이 초록으로 닫히지 못한 채 남은 검증입니다.',
  'dash.ver.head.coverage.tip':
    'diff hunk 중 도입 시점 이후에 통과 가드가 실행된 비율입니다.\n' +
    '미커버 = 변경 후 어떤 가드도 통과로 확인해 주지 않은 코드입니다.',
  'dash.ver.kind.tip':
    '가드 종류별 결과 구성(100% 기준).\n' +
    '회색 비중이 큰 종류는 exit code가 자주 가려진다는 뜻입니다.',
  'dash.ver.flow.tip':
    '가드 전체의 행방: 측정 → 통과 / 실패, 실패는 복구 / 방치로, 판정불가는 원인별로 나뉩니다.',
  'dash.ver.rhythm.tip':
    'x = 세션 진행률(시간 기준), 점 하나가 가드 실행 하나이고 색이 결과입니다.\n' +
    '빨강 뒤에 초록이 따라오는 리듬, 끝에 몰린 실행, 회색 연속이 그대로 보입니다.',
  'dash.ver.cov.tip':
    '세션별 커버(초록) / 미커버(앰버) diff hunk.\n' +
    '미커버가 많은 세션부터 확인할 가치가 있습니다.',
  'dash.eyebrow': '프로젝트 대시보드',
  'dash.allProjects': '전체 프로젝트',
  'dash.projectLabel': '프로젝트',
  'dash.windowLabel': '기간',
  'dash.window.30d': '30일',
  'dash.window.90d': '90일',
  'dash.window.all': '전체',
  'dash.sessionCount': (n: number) => `세션 ${n}개`,
  'dash.truncated': (a: { n: number; m: number }) => `전체 ${a.m}개 중 최근 ${a.n}개 표시 (limit)`,
  'dash.empty': '이 기간에 세션이 없습니다.',
  'dash.emptyHint': '먼저 transcript를 수집하세요: wimcc ingest --all',
  'dash.error': 'series를 불러오지 못했습니다.',
  'dash.loading': 'series 불러오는 중…',
  'dash.outcome.passed': 'passed',
  'dash.outcome.failed': 'failed',
  'dash.outcome.unknown': 'unknown',
  'dash.cost.total': (v: string) => `합계 ≈$${v}`,
};
