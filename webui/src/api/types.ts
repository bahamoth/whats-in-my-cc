export type Envelope<T> = { meta: { generated_at: string }; data: T };

export type SessionListItem = {
  session_id: string;
  first_observed_at: string;
  last_observed_at: string;
  event_count: number;
  source_uris: string[];
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

export type SessionDetail = {
  session_id: string;
  summary: {
    event_count: number;
    by_kind: Record<string, number>;
    first_observed_at: string;
    last_observed_at: string;
  };
  events: ObservedEventDto[];
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
};
