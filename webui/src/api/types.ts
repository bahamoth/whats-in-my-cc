export type Envelope<T> = { meta: { generated_at: string }; data: T };

export type SessionListItem = {
  session_id: string;
  first_observed_at: string;
  last_observed_at: string;
  event_count: number;
  source_uris: string[];
  /** slice-7 — per-kind row counts. May be absent on older servers. */
  by_kind?: Record<string, number>;
  /** S6 (UX 재설계) — identifiability facets. All optional: absent on older
   * servers, and individually null when the session lacks the source data
   * (no hook slug, no text assistant turn, no cwd, no user prompt). */
  project?: string;
  model?: string;
  slug?: string;
  first_user_message_preview?: string;
  /** Teammate 세션 식별 (2026-07-03) — named Agent 스폰이 만드는 별도 최상위
   * 세션의 envelope 필드. 팀메이트가 아닌 세션·구버전 서버에는 없다. */
  agent_name?: string | null;
  team_name?: string | null;
};

/** The telemetry facet, extracted from an OTel span at ingest (`TelemetryFacet`
 *  in `src/model/observed.rs`). Delivered as a SIBLING field of `payload` on the
 *  events DTO (`observed_to_dto` in `src/api/routes.rs`), not nested inside it.
 *  `attributes`/`resource` are FLAT key→value objects: the backend's `flatten_kv`
 *  unwraps the OTLP `[{key,value:{stringValue|intValue|…}}]` array into
 *  `{ "duration_ms": 7521, "input_tokens": 6, … }`. */
export type TelemetryFacetDto = {
  span_name?: string;
  span_kind?: string | null;
  status_code?: string | null;
  status_message?: string | null;
  start_unix_nano?: number;
  end_unix_nano?: number;
  attributes?: Record<string, unknown>;
  resource?: Record<string, unknown>;
  scope_name?: string | null;
  scope_version?: string | null;
};

/** 서버(core 분류기 `src/insight/event_tags.rs`)가 tool_call 이벤트에 싣는
 *  verb.object 태그 (loop-foundations 2026-06-12). 분류는 결정론 측정이라
 *  Rust가 소유한다 — UI는 표현(칩 색)과 집계만 담당. */
export type EventTagDto = {
  value: string | null;
  disposition: 'tagged' | 'control' | 'unmatched' | 'noise';
  /** untagged 루프 집계 키 — Bash 첫 토큰 | `"tool sub"` | 확장자 | basename. */
  token: string | null;
  /** 표시용 — 선행 제어 세그먼트를 제거한 명령 또는 file_path. */
  display: string | null;
};

export type ObservedEventDto = {
  event_id: string;
  raw_event_id: string;
  session_id: string;
  event_uuid: string | null;
  parent_uuid: string | null;
  observed_at: string;
  actor: string;
  kind: string;
  subkind: string | null;
  tool_use_id: string | null;
  tool_name: string | null;
  request_id?: string | null;
  message_id?: string | null;
  turn_id: string | null;
  is_sidechain: boolean | number;
  /** subagent jsonl의 top-level `agentId` — 병렬 서브에이전트 구분 키. 비-subagent
   *  이벤트는 null 또는 ''(NULL TEXT 컬럼의 row 매핑 관례)로 온다. */
  agent_id?: string | null;
  /** Workflow run id for Workflow-tool-spawned subagents, from the file path
   *  `…/subagents/workflows/<runId>/`. The deterministic group key for a workflow
   *  fan-out. null/absent for non-workflow events. */
  workflow_run_id?: string | null;
  is_meta: boolean | number;
  /** OTel span events carry their extracted span data here (span_name +
   *  flat attributes). Absent on non-span events. C4 (Tier 3-1): span name
   *  and llm-request metrics are read from this facet, not `payload.raw_span`
   *  (which was the removed double-store). */
  telemetry?: TelemetryFacetDto | null;
  /** tool_call 이벤트에만 존재 — 그 외 kind는 null. */
  tag?: EventTagDto | null;
  payload: unknown;
};

/** S8 (UX 재설계) — per-turn token sums (present only on turns with correlated
 *  usage rows). Drives the KPI strip's intra-session sparklines. */
export type TurnTokensDto = {
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
};

/** Dogfood 2026-06-12 — one entry per user prompt turn. S8 adds `tokens`. */
export type TurnRollupDto = {
  turn_id: string;
  first_observed_at: string;
  last_observed_at: string;
  tool_call_total: number;
  tool_histogram: Record<string, number>;
  tag_histogram: Record<string, number>;
  files_edited: string[];
  tokens?: TurnTokensDto;
};

export type TurnRollupResponse = {
  session_id: string;
  turns: TurnRollupDto[];
  file_churn: Array<{ file_path: string; turn_count: number; edit_count: number }>;
};

/** Slice-9 — `events` removed. Use `GET /v1/sessions/:id/events?...` for the
 *  cursor-paged window. See {@link SessionEventsResponse}. */
export type SessionDetail = {
  session_id: string;
  /** B-6c — teammate 세션의 agent 타입("Explore" 등). 비팀메이트는 부재. */
  agent_setting?: string;
  summary: {
    event_count: number;
    by_kind: Record<string, number>;
    first_observed_at: string;
    last_observed_at: string;
  };
};

/** Slice-9 — windowed events response. `next_cursor: null` means the window
 *  reaches the session's live tip; SSE supersedes further appends. */
export type SessionEventsResponse = {
  events: ObservedEventDto[];
  prev_cursor: string | null;
  next_cursor: string | null;
  /** 필터 매칭 총수 — 필터 파라미터가 하나라도 있을 때만 포함(§1.2). */
  matched_count?: number;
};

/** Backend serialises evidence refs as bare ULID `event_id` strings — the
 *  deterministic L1 extractors emit only these. (The structured node/edge
 *  shape was dropped with the graph layer; the object branch is retained
 *  only as a permissive fallback for forward-tolerance.) */
export type EvidenceRef =
  | string
  | {
      kind: 'event' | string;
      event_id?: string;
      [key: string]: unknown;
    };

export type SignalDto = {
  signal_id: string;
  schema_version: string;
  session_id: string;
  detector: string;
  subkind: string | null;
  summary: string;
  evidence_refs: EvidenceRef[];
  facts: Record<string, unknown>;
  provenance: Record<string, unknown>;
  created_at: string;
};

export type VerificationRunDto = {
  verification_run_id: string;
  schema_version: string;
  session_id: string;
  source: string;
  command: string;
  command_kind: string;
  trigger_event_id: string;
  trigger_tool_use_id: string | null;
  status: 'passed' | 'failed' | 'skipped' | string;
  /** `status`가 어떻게 결정됐는지의 출처(migration 0022): 'measured' = exit code
   *  직접 관측, 'estimated' = 도구 출력 텍스트 휴리스틱(성공/실패 요약 패턴),
   *  'unknown' = 판정 불가(piped로 가려짐·미실행 disposition 등). pre-0022 행은
   *  null. 주의 — 검증 카드 배지의 measured/mixed 어휘와는 다른 축이다: 배지는
   *  detection_basis·status_basis(run 집합의 감지·관측 방식)에서 파생되고, 이
   *  필드는 개별 run의 status 값 자체의 신뢰 출처다. */
  status_provenance: 'measured' | 'estimated' | 'unknown' | string | null;
  detection_basis: 'known_tool' | 'test_keyword' | string;
  /** 측정 불가 사유 포함: 'background'(출력/exit code가 이 이벤트에 없음) ·
   *  'user_rejected'/'policy_denied'/'cancelled'(도구가 실행되지 않음). */
  status_basis:
    | 'exit'
    | 'piped'
    | 'background'
    | 'user_rejected'
    | 'policy_denied'
    | 'cancelled'
    | string;
  started_at: string;
  ended_at: string | null;
  exit_code: number | null;
  failure_summary: string | null;
  covered_diff_hunk_ids: string[];
};

export type DiffHunkDto = {
  diff_hunk_id: string;
  session_id: string;
  file_path: string;
  change_type: string;
  line_range_after_start: number | null;
  line_range_after_end: number | null;
  introduced_by_event_id: string;
  introduced_by_tool_use_id: string | null;
  patch_preview: string;
  lines_added: number;
  lines_removed: number;
  user_modified: boolean;
};


/** §2.2 — 이 모델에 적용된 per-Mtoken USD 단가(공개 가격표 ESTIMATE). */
export type ModelRatesDto = {
  input_per_mtok: number;
  cache_creation_per_mtok: number;
  cache_read_per_mtok: number;
  output_per_mtok: number;
};

export type ModelUsageDto = {
  model: string;
  assistant_events: number;
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  /** Per-model public-pricing ESTIMATE (USD); 0 when unpriced. */
  estimated_cost_usd: number;
  priced: boolean;
  /** 적용 단가 — 미가격 모델은 null. SSOT는 백엔드 pricing.json. */
  rates: ModelRatesDto | null;
};

export type SessionUsageDto = {
  session_id: string;
  assistant_events: number;
  user_turns: number;
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  billed_tokens: number;
  /** Public-pricing ESTIMATE (USD) — NOT actual billing. */
  estimated_cost_usd: number;
  /** "estimate_public_pricing" — drives the 추정 badge. */
  cost_basis: string;
  pricing_version: string;
  models_without_pricing: string[];
  by_model: ModelUsageDto[];
};

/** insight-redesign #6 — one baseline metric's quantile triple. All null
 *  when no session in the scope has usage_facet rows for the metric. */
export type BaselineStat = {
  p25: number | null;
  median: number | null;
  p75: number | null;
  /** PR-3 — 이 지표 분포에 들어간 세션 수(지표별 게이트가 상이). n<3 → 표본 부족. */
  n: number;
};

/** insight-redesign #6 + PR-3 §3a — cross-session usage baseline. Median
 *  (+ p25/p75) of each key metric across the stored sessions in scope. The UI
 *  renders a measured session value as a delta against `*.median`
 *  ("vs your median"). */
export type UsageBaselineDto = {
  session_count: number;
  /** "project" | "store" — session_id 스코프 해석 결과(관측 사실). */
  scope: string;
  project: string | null;
  cache_hit_ratio: BaselineStat;
  billed_tokens: BaselineStat;
  assistant_events: BaselineStat;
  output_tokens: BaselineStat;
  verification_pass_rate: BaselineStat;
  tool_failure_count: BaselineStat;
  estimated_cost_usd: BaselineStat;
};

/** `GET /v1/sessions/:id/tasks` — per-task summary (TaskCreate/TaskUpdate
 *  correlated + work-span window aggregations). Computed server-side by the
 *  `task_summary` aggregator. The task list (glance) renders these; expanding a
 *  row jumps the replay to its in_progress transition's `event_id`. */
export type TaskTransitionDto = { status: string; at_ms: number; event_id: string };
export type TaskVerifDto = { passed: number; failed: number; unknown: number; not_executed: number };
export type TaskTokensDto = { input: number; output: number; cache_creation: number; cache_read: number };
export type TaskHistEntryDto = { tag: string; count: number };
export type TaskDto = {
  task_id: string;
  subject: string;
  description: string | null;
  active_form: string | null;
  /** TaskCreate event_id. */
  event_id: string;
  created_at_ms: number;
  status: string;
  transitions: TaskTransitionDto[];
  duration_ms: number | null;
  work_duration_ms: number | null;
  saw_in_progress: boolean;
  // work-span window aggregations (null unless there is an in_progress span):
  activity_count: number | null;
  tag_histogram: TaskHistEntryDto[];
  lines_added: number | null;
  lines_removed: number | null;
  verification: TaskVerifDto | null;
  tokens: TaskTokensDto | null;
};

/** `GET /v1/plugins` — a marketplace-installed plugin, resolved from the
 *  `claude` CLI (see Rust `src/plugins.rs`). The detail view matches an MCP tool
 *  call's server name against `mcp_servers` to show this reference card. */
export type PluginDto = {
  /** `plugin@marketplace`. */
  id: string;
  plugin: string;
  marketplace: string;
  /** `official` | `public` | `personal` | `unknown`. */
  provenance: string;
  scope: string;
  enabled: boolean;
  mcp_servers: string[];
  description: string | null;
};

export type SessionMetricsDto = {
  session_id: string;
  tool_call_total: number;
  tool_failure_count: number;
  /** passed + failed + unknown + not_executed. 측정 비율은 passed/(passed+failed)를 쓴다(분모로 total 사용 금지). */
  verification_total: number;
  verification_passed: number;
  verification_failed: number;
  /** 실행됐으나 결과를 읽을 수 없음(piped/요약없음). 실패 아님 — 분모 제외. */
  verification_unknown: number;
  /** 실행 자체가 안 됨(disposition: 거부/차단/취소/백그라운드). unknown과 별개 축
   *  — 비실행분이 unknown을 부풀리지 않도록 분리(2026-06-23). */
  verification_not_executed: number;
  context_bloat_count: number;
  /** 사용자가 permission 프롬프트에서 거부한 호출 수 — 실행 안 됨(실패 아님). */
  tool_user_rejected: number;
  /** PreToolUse hook이 차단한 호출 수 — 실행 안 됨(실패 아님). */
  tool_policy_denied: number;
  /** 병렬 tool call 취소 수 — 실행 안 됨(실패 아님). */
  tool_cancelled: number;
  /** 백그라운드 실행 전환 수 — 해당 tool_result content는 실제 출력이 아님. */
  tool_backgrounded: number;
  /** system/turn_duration 레코드의 durationMs 합(ms). 평균은 소비자가
   *  turn_duration_count로 나눠 계산한다(F1: count·합만 제공). */
  turn_duration_ms_total: number;
  turn_duration_count: number;
  /** system/api_error 레코드 수 (예: 529 overloaded → retry). */
  api_error_count: number;
  /** api_error 중 /error/status==429 — Anthropic API rate_limit_error (2026-07-04). */
  api_rate_limit_count: number;
  /** usage facet 세션 합계 — 할당량(rate limit budget)은 관측면에 없어 사용량만. */
  input_tokens: number;
  output_tokens: number;
  cache_read_input_tokens: number;
  cache_creation_input_tokens: number;
  /** 공개 가격표 추정 비용(USD, ≈) — 실비 아님·하한 추정(가격표 밖 모델 제외). */
  estimated_cost_usd: number;
  /** system/compact_boundary 레코드 수 — 컨텍스트 압축 횟수. */
  compact_boundary_count: number;
  /** `... [N characters truncated] ...` 잘림 마커를 포함한 tool_result 수 —
   *  CC 캡처 채널에서 출력이 잘린 사실의 측정값. */
  tool_result_truncated_count: number;
  /** `[Request interrupted by user`로 시작하는 user_message 수. */
  user_interruption_count: number;
  detector_firing: Record<string, number>;
  /** §3d — 세션 내 LLM 요청 p50(전수 계산). p50=null이면 미측정. n<3이면 배지
   *  대신 표본 부족. */
  llm_request_p50: LlmRequestP50Dto;
};

/** §3d — 단일 지표의 세션 내 p50 + 표본 수. `p50`이 null이면 미측정(0 아님). */
export type P50StatDto = { p50: number | null; n: number };

/** §3d — LLM 요청 4종(ttft/duration/output_tokens/cost) p50. */
export type LlmRequestP50Dto = {
  ttft_ms: P50StatDto;
  duration_ms: P50StatDto;
  output_tokens: P50StatDto;
  cost_usd: P50StatDto;
};

/** `/v1/sessions/:id/fingerprint` · `/v1/metrics` row 내장 — 세션 환경
 *  fingerprint (distinct 정렬 목록들). 코호트 경계 판정의 재료. */
export type SessionFingerprintDto = {
  session_id: string;
  models: string[];
  cc_versions: string[];
  git_branches: string[];
  cwds: string[];
  entrypoints: string[];
  /** 4차 개정 — 관측된 플러그인/MCP 서버 집합(개입 차원). 구서버 응답엔 부재. */
  plugins?: string[];
  /** instruction 전향 관측 — (source, sha256) 집합. 구서버·미관측 세션은 부재. */
  instructions?: Array<{ source: string; hash: string }>;
};

/** `/v1/metrics` — 세션 횡단 series의 세션 한 행. */
export type SessionSeriesRowDto = {
  session_id: string;
  first_observed_at: string;
  last_observed_at: string;
  event_count: number;
  metrics: SessionMetricsDto;
  fingerprint: SessionFingerprintDto;
};

/** `GET /v1/metrics?project=&from=&to=&limit=` 응답 data. limit 절단은
 *  matched_count로 노출된다(silent cap 금지 — series.rs). */
export type MetricsSeriesDto = {
  sessions: SessionSeriesRowDto[];
  session_count: number;
  matched_count: number;
};

export type RawEventResponse = {
  schema_version: string;
  event_id: string;
  session_id: string;
  source: {
    kind: string;
    file_path: string;
    line_no: number;
    ingested_at: string;
  };
  record: unknown;
  record_type: string;
  redaction_state: 'none' | 'partial' | 'redacted' | string;
  telemetry?: {
    span_name?: string;
    span_kind?: string | null;
    status_code?: string | null;
    status_message?: string | null;
    attributes?: Record<string, unknown>;
    resource?: Record<string, unknown>;
  } | null;
};

/** 2026-07-04 검증 탭 — GET /v1/verification/summary. 파생 정의의 SSOT는
 * 백엔드 insight::verification_summary (tests/api_verification_summary.rs). */
export type VerificationSummaryDto = {
  total: number;
  measured: number;
  passed: number;
  failed: number;
  unknown: number;
  unknown_piped: number;
  unknown_other: number;
  not_executed: number;
  by_kind: Array<{
    kind: string;
    passed: number;
    failed: number;
    unknown: number;
    not_executed: number;
  }>;
  failures: { recovered: number; abandoned: number };
  rhythm: Array<{
    session_id: string;
    guards: number;
    passed: number;
    runs: Array<{ pct: number; status: string }>;
  }>;
  coverage: {
    covered: number;
    total: number;
    by_session: Array<{ session_id: string; covered: number; total: number }>;
  };
};

/** instruction 전향 관측 — 세션의 관측 목록(시간순). */
export type InstructionObservationDto = {
  source: string;
  path: string;
  content_sha256: string;
  observed_at: string;
};

export type InstructionSnapshotDto = {
  content_sha256: string;
  content: string;
  first_observed_at: string;
};

/** `/v1/health`의 `version` 블록 — 스펙 2026-07-17 §4. health는 Envelope 미사용
 *  (원시 JSON). `latest`는 조회 실패/미조회 시 null. */
export interface HealthVersion {
  current: string;
  latest: string | null;
  update_available: boolean;
}
