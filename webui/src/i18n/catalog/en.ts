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
  'detail.taskBoard.title': 'Tasks',
  'detail.taskBoard.done': 'done',
  'detail.taskBoard.noInProgress': 'no in_progress',
  'detail.plugin.title': 'Plugin',
  'detail.plugin.configured': 'Directly-configured MCP server (not a marketplace plugin).',
  'detail.plugin.connector': 'Official Anthropic integration (managed connector / extension).',

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

  // Analysis panel
  'analysis.empty': 'No metrics to analyze.',
  'analysis.sessionMetrics': 'Session metrics',
  'analysis.toolFailures': 'Tool failures',
  'analysis.verificationPassed': 'Verification passed (measured)',
  'analysis.unmeasuredInline': (n: number) => ` · ${n} unmeasured`,
  'analysis.noMeasurement': 'Not measured',
  'analysis.contextBloatCount': 'Context bloat count',
  'analysis.detectorDistribution': 'Detector signal distribution',
  'analysis.noSignals': 'No signals detected',
  'analysis.reReadLabel': (a: { file: string; count: string }) => `${a.file} · ${a.count} reads`,

  // Detail — event provenance badge
  'detail.provenance.native': 'Native',
  'detail.provenance.derived': 'Derived',

  // Detail — InsightTab
  'detail.insightTab.nativeTitle': 'Claude Code native observation',
  'detail.insightTab.derivedTitle': 'wimcc derived data',
  'detail.insightTab.copyTitle': (a: { field: string; value: string }) =>
    `${a.field}: ${a.value} — click to copy`,
  'detail.insightTab.jumpToEvidence': 'Jump to evidence event',
  'detail.insightTab.evidenceHint': '↳ evidence',
  'detail.insightTab.whatTitle': 'What — what it did',
  'detail.insightTab.howTitle': 'How — metrics',

  // Detail — WhatSection
  'detail.what.errorBadge': 'Error',
  'detail.what.truncNote': (chars: string) =>
    `… truncated after ${chars} chars — full text in the Raw tab`,
  'detail.what.thinkingNotRecorded': 'Reasoning body is not recorded (signature only)',
  'detail.what.rawFallback': 'See the Raw tab for the source',

  // Detail — ResponseMetricsPanel
  'detail.response.title': 'Reasoning · response metrics',
  'detail.response.note':
    'The reasoning content is not recorded in the transcript (only an encrypted signature exists). ' +
    'Below are the measured metrics for this response (LLM request).',
  'detail.response.empty': "Couldn't find this response's metrics in the current window.",
  'detail.response.warn': 'This response shows truncation / retry / failure signals.',

  // Metrics rows (shared by EntityMetricsPanel + ResponseMetricsPanel)
  'metric.uncollected': 'Metrics not collected',
  'metric.attemptCount': (n: number) => `${n}×`,
  'metric.yes': 'yes',
  'metric.no': 'no',
  'metric.group.toolExec': 'Tool execution',
  'metric.group.hookExec': 'Hook execution',
  'metric.group.llmActivity': 'LLM activity',
  'metric.group.tokens': 'Tokens',
  'metric.group.cost': 'Cost',
  'metric.label.duration': 'Duration',
  'metric.label.result': 'Result',
  'metric.label.decisionSource': 'Decision source',
  'metric.label.ioSize': 'Input/result size',
  'metric.label.sequence': 'Sequence',
  'metric.label.hookEvent': 'Hook event',
  'metric.label.command': 'Command',
  'metric.label.ttft': 'Time to first token (ttft)',
  'metric.label.stopReason': 'Stop reason',
  'metric.label.attempts': 'Attempts',
  'metric.label.success': 'Success',
  'metric.label.model': 'Model',
  'metric.label.querySource': 'Request source',
  'metric.label.outputTokens': 'Output tokens',
  'metric.label.outputSpeed': 'Output speed',
  'metric.label.inputTokens': 'Input tokens',
  'metric.label.cacheReads': 'Cache reads',
  'metric.label.cacheCreation': 'Cache creation',
  'metric.label.billedCost': 'Billed cost',
  'metric.tip.outputTokens':
    'Tokens generated by this response. Thinking tokens are included here — the amount the model produced.',
  'metric.tip.inputTokens':
    'New (uncached) input tokens sent with this request. Most of the context is usually reused as cache reads.',
  'metric.tip.cacheReads':
    'Tokens reused from the prompt cache. Higher means most of the context was reused from cache — lowering cost and latency.',
  'metric.tip.cacheCreation':
    'Tokens newly written to the cache this time. Reused as cache reads from the next request onward.',
  'metric.tip.billedCost':
    'The measured cost of this request (USD), reported by Claude Code (api_request_log) — different from a token × public-rate estimate.',
  'metric.tip.outputSpeed':
    'Generation throughput — output tokens ÷ request duration (seconds). Thinking tokens count as output too.',
  'metric.tip.querySource':
    'Who sent this request. The main thread (user conversation) or a subagent (general-purpose · Explore, etc.) — who called it.',
  'metric.tip.decisionSource':
    'How this tool execution was allowed. config = auto-allowed by settings, user = approved by the user, etc. — the source of the permission decision.',
  'metric.tip.ioSize':
    'The size (bytes) of the input sent to the tool and the result it returned. A larger result takes up more context.',

  // Stream — shared
  'stream.reasoning': 'Reasoning',
  'stream.notification': 'Notification',
  'stream.output': 'Output',
  'stream.conclusion': 'Conclusion',
  'stream.synthesisLabel': 'Synthesis',
  'stream.inProgress': 'In progress',
  'stream.concurrentMain': (n: number) => `⟂ ${n} concurrent on main`,
  'stream.laneExpandTitle': (label: string) => `${label} — click to expand`,
  'stream.conversationStart': 'Conversation start',
  'stream.workflow.agentN': (n: number) => `Agent ${n}`,

  // Stream — autoscroll
  'stream.autoscroll.label': 'Auto-scroll',
  'stream.autoscroll.disableAria': 'Turn off auto-scroll',
  'stream.autoscroll.enableAria': 'Turn on auto-scroll and jump to latest',

  // Stream — scaffold group
  'stream.scaffold.chip': 'Commands · skills',
  'stream.scaffold.sourcePlus': (n: number) => `+${n} sources`,

  // Stream — thinking marker
  'stream.thinking.aria': 'Reasoning — click to show response metrics (content not in transcript)',
  'stream.thinking.title':
    'The reasoning content is not recorded in the transcript (only a signature). Click to see response metrics.',
  'stream.thinking.warnAria': 'Abnormal response',

  // Stream — end cards
  'stream.endCard.label': 'End',
  'stream.endCard.jumpToNotification': 'Jump to the end-notification source',
  'stream.workflowEndCard.label': 'Workflow end',
  'stream.workflowEndCard.result': 'Result',

  // Stream — untagged Bash panel
  'stream.untagged.jumpTitle': "Jump to this command's card",
  'stream.untagged.jumpLabel': 'To card ↗',

  // Stream — legend
  'stream.lane.user': 'User',
  'stream.lane.scaffold': 'Scaffold',
  'stream.lane.tool': 'Tool',
  'stream.lane.thinking': 'Reasoning',
  'stream.lane.batch': 'Batch',
  'stream.lane.workflow': 'Workflow',
  'stream.legend.aria': 'Stream legend',
  'stream.legend.move': 'move',
  'stream.legend.nextError': 'next error',
  'stream.legend.close': 'Close legend',
  'stream.legend.show': 'Legend',

  // Stream — message card
  'stream.msg.summary': 'Summary',
  'stream.msg.viewRaw': 'View raw',
  'stream.msg.viewMarkdown': 'View markdown',
  'stream.msg.bgTitle': (n: number) =>
    `${n} background subagent(s) were running while this message progressed (this message itself is not background)`,
  'stream.msg.bgRunning': (n: number) => `⟂ ${n} background running`,
  'stream.msg.done': 'Done',
  'stream.msg.jumpToCommand': 'Jump to the original command',
  'stream.msg.commandJump': '↳ command',
  'stream.msg.openSession': 'open session →',
  'stream.msg.openTeammateSession': "Open this teammate session's replay",
  'team.lead': 'lead',
  'team.agentType': 'agent type',
  'team.teammates': (n: number) => `${n} teammates`,
  'stream.msg.jumpToDispatch': 'Jump to the Agent dispatch that spawned this teammate',
  'stream.msg.dispatch': 'dispatch',
  'stream.msg.showMore': 'Show more',
  'stream.msg.collapse': 'Collapse',

  // Stream — batch group
  'stream.batch.chip': 'Parallel batch',

  // Stream — subagent group
  'stream.subagent.messages': (n: number) => `${n} messages`,
  'stream.subagent.tools': (n: number) => `${n} tools`,
  'stream.subagent.done': '✓ done',
  'stream.subagent.running': '● running',
  'stream.subagent.jumpToTask': 'Jump to the calling Task',

  // Stream — workflow group
  'stream.workflow.chip': 'Workflow',
  'stream.workflow.jumpTitle': 'Jump to the Workflow call that spawned this workflow',
  'stream.workflow.jumpAria': 'Jump to the Workflow call',
  'stream.workflow.call': 'Call',
  'stream.workflow.maxConcurrency': 'Max parallel',
  'stream.workflow.longest': 'Longest',
  'stream.workflow.median': 'Median',
  'stream.workflow.incomplete': 'Incomplete',

  // Stream — query source (who issued the LLM request)
  'stream.querySource.mainThread': 'Main thread',
  'stream.querySource.subagent': (name: string) => `Subagent · ${name}`,

  // Project dashboard (B-1)
  'nav.dashboard': 'Dashboard',
  'dash.tab.overview': 'Overview',
  'dash.tab.verification': 'Verification',
  'dash.head.pass': 'Verification pass',
  'dash.head.cost': 'Estimated cost',
  'dash.head.rate': 'Blended unit rate',
  'dash.head.hit': 'Cache hit',
  'dash.head.toolfail': 'Tool failure rate',
  'dash.head.noCompare': 'no comparison (all window)',
  'dash.head.costBasis': 'public price list ≈ · floor',
  'dash.head.ratePer': 'per 1M billed tokens',
  'dash.head.hitBasis': 'window total of input context',
  'dash.head.toolfailOf': (a: { fails: number; calls: string }) => `${a.fails} / ${a.calls} calls`,
  'dash.head.guards': (n: number) => `${n} guards in window`,
  'dash.observed': 'Observed changes',
  'dash.observed.modelFirst': (a: { date: string; model: string }) =>
    `${a.date} ${a.model} first observed`,
  'dash.observed.ccSpan': (a: { from: string; to: string; count: number }) =>
    a.count > 1 ? `Claude Code ${a.from} → ${a.to} · ${a.count} transitions` : `Claude Code ${a.from} → ${a.to}`,
  'dash.observed.topSignals': (a: { name: string; n: number }) => `most signals ${a.name} (${a.n})`,
  'dash.ver.error': 'Failed to load the verification summary.',
  'dash.daily.ver.title': 'Verification — daily',
  'dash.daily.ver.zeroGuards': (n: number) => `sessions with 0 guards: ${n}`,
  'dash.daily.ver.badge': (a: { n: number; m: number }) => `${a.n} guards · ${a.m} passed`,
  'dash.daily.cost.title': 'Daily cost · signals',
  'dash.daily.cost.desc': 'bar height = estimated cost · bar color = signals that day',
  'dash.tt.noSessions': 'no sessions',
  'dash.tt.noGuards': 'no guards observed',
  'dash.tt.signals': 'signals',
  'dash.marker.first': (m: string) => `${m} first observed`,
  'dash.cohort.compareTitle': (label: string) => `Cohort compare — around ${label}`,
  'dash.cohort.introduced': (m: string) => `${m} introduced`,
  'dash.cohort.retired': (m: string) => `${m} retired`,
  'dash.cohort.beforeAfter': (a: { b: number; a: number }) => `before ${a.b} · after ${a.a} sessions`,
  'dash.cohort.lowSample': 'low sample',
  'dash.cohort.ccAlso': 'Claude Code version also changed at this boundary — effects are not separable',
  'dash.cohort.sigPerSession': 'Signals / session',
  'dash.cohort.before': 'before',
  'dash.cohort.after': 'after',
  'dash.cohort.basis': 'measured values only',
  'dash.lane.title': 'Session timeline',
  'dash.lane.desc': 'cards at actual dates — click opens replay',
  'dash.lane.notMeasured': 'model not observed',
  'dash.lane.sig': (n: number) => `${n} signals`,
  'dash.scatter.title': 'Session distribution — billed tokens × signal density',
  'dash.scatter.desc': 'x billed tokens (log) · y signals per 100 events · size cost · color primary model',
  'dash.scatter.median': 'dashed = median',
  'dash.scatter.x': 'billed tokens (M)',
  'dash.scatter.y': 'signals / 100 events',
  'dash.scatter.click': 'click → replay',
  'dash.ver.notExec': 'not executed',
  'dash.ver.node.guards': 'guards',
  'dash.ver.node.measured': 'measured',
  'dash.ver.node.recovered': 'recovered',
  'dash.ver.node.abandoned': 'failed & left',
  'dash.ver.node.piped': 'pipe-masked',
  'dash.ver.node.other': 'other',
  'dash.ver.head.guards': 'Guard runs',
  'dash.ver.head.measured': 'Measured (exit code)',
  'dash.ver.head.unknownChip': (n: number) => `undecidable ${n}`,
  'dash.ver.head.unknownSplit': (a: { p: number; o: number }) => `pipe-masked ${a.p} · other ${a.o}`,
  'dash.ver.head.passed': 'Passed (of measured)',
  'dash.ver.head.abandoned': 'Failed & left',
  'dash.ver.head.abandonedOf': (n: number) => `of ${n} failed`,
  'dash.ver.head.abandonedNote': 'no later pass of the same kind in the session',
  'dash.ver.head.coverage': 'Change coverage',
  'dash.ver.head.coverageNote': (n: number) => `of ${n} diff hunks`,
  'dash.ver.kind.title': 'Results by guard kind',
  'dash.ver.kind.desc': '100% stacked · right = count',
  'dash.ver.flow.title': (n: number) => `Where ${n} guards went`,
  'dash.ver.flow.desc': 'measured → result → failure recovery',
  'dash.ver.rhythm.title': 'Guard run rhythm',
  'dash.ver.rhythm.desc': 'run position over session progress — top sessions by guard count',
  'dash.ver.rhythm.axis': 'x = session progress (time)',
  'dash.ver.rhythm.meta': (a: { g: number; p: number }) => `${a.g} guards · ${a.p} passed`,
  'dash.ver.cov.title': 'Change coverage — hunks touched by a passing guard',
  'dash.ver.cov.desc': 'covered / uncovered diff hunks · top sessions by hunk count',
  'dash.ver.cov.overall': (a: { pct: number; n: number }) => `overall ${a.pct}% · uncovered ${a.n}`,
  'dash.ver.cov.uncovered': (n: number) => `uncovered ${n}`,
  'dash.range.last30': 'Last 30 days',
  'dash.range.last90': 'Last 90 days',
  'dash.range.all': 'All time',
  'dash.range.picking': (d: string) => `${d} → pick the end date`,
  'dash.range.hint': 'Pick two dates on the calendar for a custom range.',
  'dash.cohort.secTitle': 'Cohort boundaries',
  'dash.cohort.prefixNote': 'before/after = all sessions in the window before/after the boundary',
  'dash.cohort.dim.auto': 'auto — top by exceedance',
  'dash.cohort.dim.models': 'model',
  'dash.cohort.dim.cc': 'Claude Code',
  'dash.cohort.dim.branch': 'branch',
  'dash.cohort.dim.cwd': 'cwd',
  'dash.cohort.dim.entrypoint': 'entrypoint',
  'dash.cohort.exceed': (x: number) => `top ${x}% vs random splits`,
  'dash.cohort.noneAuto':
    'no boundary passes the exceedance gate in this window — pick a dimension to browse all boundaries',
  'dash.cohort.alsoDims': (d: string) => `${d} also changed at this point — effects are not separable`,
  'dash.head.pass.tip':
    'Pass rate over measured guards: passed ÷ (passed + failed).\n' +
    'A guard is a test / build / lint / format run observed in the window.\n' +
    'Undecidable runs (masked exit codes) stay out of the denominator.\n' +
    'The delta chip compares against the previous window of the same length.',
  'dash.head.cost.tip':
    'Window total of estimated cost: public price list × usage tokens.\n' +
    'This is a floor, not a bill — subscription discounts and service tiers are invisible locally.\n' +
    'Cache reads are free and excluded.',
  'dash.head.rate.tip':
    'Blended unit rate = estimated cost ÷ billed tokens (input + cache write + output) × 1M.\n' +
    'Rises when expensive models take a larger share of the window.',
  'dash.head.hit.tip':
    'Share of input context served from cache: cache read ÷ (input + cache write + cache read).\n' +
    'Higher means the same context was reused cheaply.',
  'dash.head.toolfail.tip':
    'Failed tool calls ÷ all tool calls in the window.\n' +
    'A deterministic count; severity is not judged.',
  'dash.observed.tip':
    'Every item is derived from observations:\n' +
    '· model first observed — the day a model first appears in this window\n' +
    '· Claude Code — first → last version with the number of transitions\n' +
    '· most signals — the session with the largest process-signal total',
  'dash.daily.ver.tip':
    'A guard is an observed test / build / lint / format run.\n' +
    'passed / failed come from exit codes; unknown means the exit code was masked (e.g. piped commands).\n' +
    'Purple dashed lines mark model / Claude Code transitions.',
  'dash.daily.cost.tip':
    'Bar height = estimated cost of that day (public price list ≈, floor).\n' +
    'Bar color = process signals that day, green (0) through red (max) — hot days stand out.\n' +
    'Hover a bar to see the sessions behind it.',
  'dash.cohort.tip':
    'A boundary is where the session environment (fingerprint) changes — detected across 5 dimensions: model, Claude Code, branch, cwd, entrypoint.\n' +
    '"top x% vs random splits" = the metric shift at this boundary ranks in the top x% of shifts from every possible split of this window. Lower = the change stands out more.\n' +
    'The auto list shows up to 3 boundaries within the top 10% (fixed rule, spec §2).\n' +
    'before/after = all sessions in the window before/after the boundary.',
  'dash.cohort.sig.tip':
    'Average process signals per session — the sum of 6 signal kinds (tool failures, context compactions, interruptions, …).',
  'dash.lane.tip':
    'Cards sit at their actual dates; same-day sessions stack into rows.\n' +
    'Card: session name · models · estimated cost · signals · pass rate · events.\n' +
    'An amber signal count means signal density above 2× the window median.\n' +
    'Small dashed chips are idle sessions (no usage, signals, or guards).\n' +
    'Click a card to open the replay.',
  'dash.scatter.tip':
    'x = billed tokens (log scale) · y = signals per 100 events · size = estimated cost · color = primary model.\n' +
    'Dashed lines are the medians — upper right means heavy use with frequent signals.\n' +
    'Top sessions by cost or signal density carry name labels.\n' +
    'Click a dot to open the replay.',
  'dash.ver.head.guards.tip':
    'Total observed verification runs (test / build / lint / format) in the window.',
  'dash.ver.head.measured.tip':
    'Guards whose exit code was captured, so pass / fail is decidable.\n' +
    'The main cause of undecidable runs is pipe masking — `cmd | tail` hides the exit code.',
  'dash.ver.head.passed.tip': 'Passed ÷ measured (passed + failed) guards.',
  'dash.ver.head.abandoned.tip':
    'Failed guards with no later pass of the same kind before the session ended.\n' +
    'Red that never turned green.',
  'dash.ver.head.coverage.tip':
    'Share of diff hunks with a passing guard after they were introduced.\n' +
    'Uncovered = changed code no guard has confirmed since the change.',
  'dash.ver.kind.tip':
    'Result mix per guard kind (normalized to 100%).\n' +
    'A large gray share means that kind often runs with masked exit codes.',
  'dash.ver.flow.tip':
    'Where every guard went: measured → passed / failed; failures split into recovered / left; undecidable splits by cause.',
  'dash.ver.rhythm.tip':
    'x = session progress (by time), each dot is a guard run colored by result.\n' +
    'Patterns read directly: red followed by green, runs bunched at the end, gray streaks.',
  'dash.ver.cov.tip':
    'Covered (green) vs uncovered (amber) diff hunks per session.\n' +
    'Sessions with many uncovered hunks are worth checking first.',
  'dash.eyebrow': 'Project dashboard',
  'dash.allProjects': 'All projects',
  'dash.projectLabel': 'Project',
  'dash.windowLabel': 'Window',
  'dash.window.30d': '30d',
  'dash.window.90d': '90d',
  'dash.window.all': 'All',
  'dash.sessionCount': (n: number) => `${n} sessions`,
  'dash.truncated': (a: { n: number; m: number }) =>
    `showing the latest ${a.n} of ${a.m} sessions (limit)`,
  'dash.empty': 'No sessions in this window.',
  'dash.emptyHint': 'Ingest transcripts first: wimcc ingest --all',
  'dash.error': 'Failed to load the series.',
  'dash.loading': 'Loading series…',
  'dash.outcome.passed': 'passed',
  'dash.outcome.failed': 'failed',
  'dash.outcome.unknown': 'unknown',
  'dash.cost.total': (v: string) => `total ≈$${v}`,
};

export type Messages = typeof en;
export type MessageKey = keyof Messages;
