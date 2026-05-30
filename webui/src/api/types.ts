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
  turn_id: string | null;
  is_sidechain: boolean | number;
  is_meta: boolean | number;
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

export type GraphNodeDto = {
  node_id: string;
  schema_version: string;
  session_id: string;
  node_kind: string;
  started_at: string;
  ended_at: string | null;
  merge_keys: Record<string, unknown>;
  source_event_ids: string[];
  source_uris: string[];
  payload: unknown;
};

export type GraphEdgeDto = {
  edge_id: string;
  schema_version: string;
  session_id: string;
  from_node_id: string;
  to_node_id: string;
  edge_kind: 'message_reply' | 'tool_call_to_result' | string;
  origin: 'deterministic' | 'inferred' | string;
  attributes: Record<string, unknown>;
  /** slice-13. Non-null iff origin === 'inferred'. */
  inference_rule_id?: string | null;
  /** slice-13. 0.0–1.0 confidence for inferred edges. */
  confidence?: number | null;
};

export type GraphPayload = { nodes: GraphNodeDto[]; edges: GraphEdgeDto[] };

/** Backend serialises evidence refs in two shapes today: bare ULID
 *  strings (the common case, used by slice-14 deterministic extractors)
 *  and structured `{ kind, node_id|edge_id|event_id }` objects (slice-15+
 *  judges). Client code must handle both. */
export type EvidenceRef =
  | string
  | {
      kind: 'node' | 'edge' | 'event' | string;
      node_id?: string;
      edge_id?: string;
      event_id?: string;
      [key: string]: unknown;
    };

export type FindingDto = {
  finding_id: string;
  schema_version: string;
  session_id: string;
  category: string;
  subkind?: string | null;
  severity: 'low' | 'medium' | 'high' | 'info' | string;
  confidence: number;
  summary: string;
  evidence_refs: EvidenceRef[];
  evidence_projection: Record<string, unknown>;
  provenance: Record<string, unknown>;
  status: string;
  created_at: string;
};

/** insight-redesign #3 — tool_failure class breakdown for a session. Only
 *  `user_visible` is headline-eligible; the other two classes are surfaced
 *  for transparency but never lumped into a `severity=high` headline. */
export type ToolFailureSummaryDto = {
  session_id: string;
  user_visible: number;
  internal_retry: number;
  benign_nonzero_exit: number;
  unclassified: number;
  total: number;
  user_visible_findings: FindingDto[];
};

export type EpisodeDto = {
  episode_id: string;
  schema_version: string;
  session_id: string;
  phase:
    | 'intake'
    | 'exploration'
    | 'diagnosis'
    | 'action'
    | 'verification'
    | 'repair'
    | 'drift'
    | 'stall'
    | string;
  start_event_id: string;
  end_event_id: string;
  started_at: string;
  ended_at: string;
  evidence_node_ids: unknown[];
  classification_basis: unknown[];
  confidence: number;
  summary: string | null;
  classifier_version: string;
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

export type FindingEvidenceResponse = {
  finding: FindingDto;
  subgraph: { nodes: unknown[]; edges: unknown[] };
  raw_source_refs: Array<{
    event_id: string;
    source_type: string;
    source_uri: string;
    redaction_state: string;
  }>;
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
