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
  is_meta: boolean | number;
  /** OTel span events carry their extracted span data here (span_name +
   *  flat attributes). Absent on non-span events. C4 (Tier 3-1): span name
   *  and llm-request metrics are read from this facet, not `payload.raw_span`
   *  (which was the removed double-store). */
  telemetry?: TelemetryFacetDto | null;
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
  detection_basis: 'known_tool' | 'test_keyword' | string;
  status_basis: 'exit' | 'piped' | string;
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
  turns: number;
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
  turns: number;
  input_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
  output_tokens: number;
  billed_tokens: number;
  cache_hit_ratio: number | null;
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
  turns: BaselineStat;
  output_tokens: BaselineStat;
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
