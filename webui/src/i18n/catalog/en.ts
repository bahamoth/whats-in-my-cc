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
  'dash.head.prevWindow': (v: string) => `previous window ${v}`,
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
    a.count > 1 ? `CC ${a.from} → ${a.to} · ${a.count} transitions` : `CC ${a.from} → ${a.to}`,
  'dash.observed.topSignals': (a: { name: string; n: number }) => `most signals ${a.name} (${a.n})`,
  'dash.ver.loading': 'Loading verification summary…',
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
  'dash.cohort.title': 'Model cohorts',
  'dash.cohort.tip':
    'The models that actually responded in each session; multiple models join with +.\n' +
    'Dashed rule — the model mix changed here\n' +
    'Dashed box — sessions with no model record (shown as continuing the previous mix)',
  'dash.cohort.models': 'model',
  'dash.cohort.ccTip':
    'The Claude Code version each session ran on.\n' +
    'Alternating shades separate adjacent version spans; sessions without a version record ' +
    'continue the previous span (dashed box).',
  'dash.cohort.cc': 'CC version',
  'dash.cohort.unknown': 'not observed yet',
  'dash.outcome.title': 'Verification outcomes',
  'dash.outcome.tip':
    'How each session\'s verification commands (tests, builds, lints) ended.\n' +
    'passed — confirmed success\n' +
    'failed — confirmed failure\n' +
    'unknown — ran, but the output was cut before a result could be read\n' +
    'Commands that never ran (rejected, cancelled) are excluded.',
  'dash.outcome.passed': 'passed',
  'dash.outcome.failed': 'failed',
  'dash.outcome.unknown': 'unknown',
  'dash.outcome.none': 'no verification runs in this window',
  'dash.multiples.title': 'Process signals',
  'dash.multiples.tip':
    'Counts of what happened during each session.\n' +
    'Example: rising tool failures with steady passed outcomes above reads as more trial and ' +
    'error that still landed.',
  'dash.metric.tool_failure_count': 'tool failures',
  'dash.metric.context_bloat_count': 'context bloat signals',
  'dash.metric.api_error_count': 'API errors',
  'dash.metric.user_interruption_count': 'user interruptions',
  'dash.metric.compact_boundary_count': 'context compactions',
  'dash.metric.tool_result_truncated_count': 'truncated tool results',
  'dash.metric.api_rate_limit_count': 'rate limits (429)',
  'dash.tokens.title': 'Token usage',
  'dash.tokens.tip':
    'Tokens each session consumed.\n' +
    'input — fresh input sent\n' +
    'cache write — input newly stored in cache\n' +
    'output — model output\n' +
    'cache read — input reused from cache (dwarfs the rest, so it gets its own strip)\n' +
    'The API does not report remaining quota, so this stops at usage.',
  'dash.tokens.empty': 'usage data is empty — the running serve predates these fields (restart with the new build) or usage facets need re-ingest.',
  'dash.eff.title': 'Efficiency',
  'dash.eff.tip':
    'Per-session usage ratios — darker cell = higher value.\n' +
    'cache%: share of input context served from cache (cache read ÷ input + cache write + cache read). Higher is cheaper.\n' +
    'out%: output share of billed tokens (output ÷ input + cache write + output). How much of the spend became output.\n' +
    '$/1M: blended cost per 1M billed tokens — rises with expensive-model mix.\n' +
    'Sessions without usage data draw no cell.',
  'dash.eff.hit.name': 'Cache hit ratio',
  'dash.eff.hit.short': 'hit%',
  'dash.eff.out.name': 'Output share of billed tokens',
  'dash.eff.out.short': 'out%',
  'dash.eff.rate.name': 'Blended cost per 1M billed tokens',
  'dash.eff.rate.short': '$/1M',
  'dash.cost.title': 'Estimated cost (≈$)',
  'dash.cost.tip':
    'Per-session cost estimated from the public price list.\n' +
    'Not a bill — subscription discounts and service tiers are not visible locally, and models ' +
    'missing from the price list are excluded, so this is a floor.',
  'dash.cost.total': (v: string) => `total ≈$${v}`,
  'dash.tokens.input': 'input',
  'dash.tokens.output': 'output',
  'dash.tokens.cacheCreation': 'cache write',
  'dash.tokens.cacheRead': 'cache read',
  'dash.axis.max': (n: number | string) => `max ${n}`,
  'dash.tab.charts': 'Charts',
  'dash.table.summary': 'Data table',
  'dash.table.session': 'session',
  'dash.table.date': 'first observed',
  'dash.table.events': 'events',
  'dash.tooltip.events': (n: number) => `${n} events`,
  'dash.openSession': (id: string) => `Open session ${id}`,
};

export type Messages = typeof en;
export type MessageKey = keyof Messages;
