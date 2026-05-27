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
};

export type GraphPayload = { nodes: GraphNodeDto[]; edges: GraphEdgeDto[] };

/** Slice-11 — M5 Finding row, surfaced by `GET /v1/sessions/:id/findings`.
 *  Mirrors the `finding` SQLite row 1:1. `evidence_refs` is raw JSON because
 *  different rule categories may attach different shapes. */
export type FindingDto = {
  finding_id: string;
  schema_version: string;
  session_id: string;
  category: string;
  severity: string;
  claim: string;
  confidence: number;
  limitations: string[];
  evidence_refs: Array<{ node_id: string; role: string }>;
  generated_at: string;
  rule_version: string;
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
