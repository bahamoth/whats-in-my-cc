export type Envelope<T> = { meta: { generated_at: string }; data: T };

export type SessionListItem = {
  session_id: string;
  first_observed_at: string;
  last_observed_at: string;
  event_count: number;
  source_uris: string[];
  /** slice-7 — per-kind row counts. May be absent on older servers. */
  by_kind?: Record<string, number>;
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
  disposition: 'tagged' | 'control' | 'unmatched';
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

/** Slice-9 — `events` removed. Use `GET /v1/sessions/:id/events?...` for the
 *  cursor-paged window. See {@link SessionEventsResponse}. */
export type SessionDetail = {
  session_id: string;
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
 *  when no session in the store has usage_facet rows for the metric. */
export type BaselineStat = {
  p25: number | null;
  median: number | null;
  p75: number | null;
};

/** insight-redesign #6 — cross-session usage baseline. Median (+ p25/p75) of
 *  each key metric across all stored sessions with usage_facet rows. The UI
 *  renders a measured session value as a delta against `*.median`
 *  ("vs your median"). */
export type UsageBaselineDto = {
  session_count: number;
  cache_hit_ratio: BaselineStat;
  billed_tokens: BaselineStat;
  assistant_events: BaselineStat;
  output_tokens: BaselineStat;
};

export type SessionMetricsDto = {
  session_id: string;
  tool_call_total: number;
  tool_failure_count: number;
  /** passed + failed + unknown. 측정 비율은 passed/(passed+failed)를 쓴다(분모로 total 사용 금지). */
  verification_total: number;
  verification_passed: number;
  verification_failed: number;
  verification_unknown: number;
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
  /** system/compact_boundary 레코드 수 — 컨텍스트 압축 횟수. */
  compact_boundary_count: number;
  /** `... [N characters truncated] ...` 잘림 마커를 포함한 tool_result 수 —
   *  CC 캡처 채널에서 출력이 잘린 사실의 측정값. */
  tool_result_truncated_count: number;
  /** `[Request interrupted by user`로 시작하는 user_message 수. */
  user_interruption_count: number;
  detector_firing: Record<string, number>;
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
