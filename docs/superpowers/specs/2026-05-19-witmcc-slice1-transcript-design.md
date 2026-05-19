# Slice-1: Transcript Vertical Slice — Design

- **Date:** 2026-05-19
- **Scope unit:** `whats-in-my-cc` — slice-1 of MVP
- **Status:** Draft for review
- **Owner:** bahamoth@ncsoft.com

## Context

`What's in My Claude Code`는 Claude Code 실행을 로컬에서 관측해 OTel-first 실행 그래프와 evidence-linked insight를 만드는 로컬 서비스다. 전체 MVP는 6개 spec 문서(`docs/00..06_*.html`)에 명시되어 있고 M0(Spec freeze)는 완료됐다. 전체 MVP는 7개 milestone, 4개 source collector(Transcript / OTel / Hook / File·Git), 7단계 pipeline, UI · Pull API · MCP Streamable HTTP 서빙 계층을 포함한다. 단일 spec/implementation plan으로 묶기에는 크다.

본 문서는 그중 **slice-1: Transcript 단일 source로 ingest → normalize → deterministic graph → 최소 Pull API**를 end-to-end로 돌리는 thin vertical slice를 정의한다. 목표는 데이터 모델·모듈 경계·schema 호환성을 실데이터로 잠가서 후속 spec(OTel/Hook/File·Git/Findings/UI/MCP/Redaction)이 redo 없이 위에 얹히게 만드는 것이다.

## Goals / Non-goals

### Goals (slice-1)
- `~/.claude/projects/**/<session-uuid>.jsonl`을 배치로 ingest해 `raw_event` + `observed_event`로 저장.
- 한 세션 단위로 deterministic edge만 가진 최소 graph(`graph_node`/`graph_edge`) 생성.
- 127.0.0.1 bind axum HTTP 서버에서 `meta`/`data` envelope을 가진 4개 GET 엔드포인트 제공.
- ULID 시간정렬 + 노드/엣지의 **해시-유도 ID**로 멱등·결정성 보장.
- 후속 spec이 깨지 않고 ALTER ADD로 확장할 수 있도록 schema에 OTel/redaction nullable 컬럼을 미리 비워둠.

### Non-goals (slice-1, 후속 spec으로 미룸)
- OTel receiver, Hook lifecycle collector, File/Git observer.
- Redaction gate, local auth token, MCP Streamable HTTP.
- UI(causal replay, why panel, source view).
- Findings/Insights 생성 엔진, inferred causal edge.
- Retention/Export, file watcher/live tail, raw payload 외부화·압축, incremental graph rebuild.

## Decisions Locked

| 결정 | 값 |
|---|---|
| 언어/런타임 | Rust (edition 2021, rust-toolchain `1.78`), tokio multi-thread |
| HTTP | axum 0.7 + tower-http 0.5 |
| DB | SQLite via sqlx 0.8 (`runtime-tokio-rustls`, `sqlite`, `macros`, `migrate`, `json`) |
| 마이그레이션 | `sqlx::migrate!("./migrations")` (refinery 배제) |
| CLI | clap 4.5 (derive) — `witmcc {init-db|ingest|serve}` |
| Time ID | `ulid` 1.1 (monotonic generator, single-task 발급) |
| 노드/엣지 ID | SHA-256 해시 유도 (ULID 아님 — 결정성 위반 회피) |
| Tracing | `tracing` + `tracing-subscriber` env-filter |
| Bind | 127.0.0.1 only; auth/redaction 없음 |
| 보안 미들웨어 | Host 헤더 화이트리스트(`127.0.0.1`/`localhost`), CORS = deny-all |

## Module Layout

단일 crate, 단일 bin. 워크스페이스/서브 crate는 slice-2 이후 분할.

```
whats-in-my-cc/
├─ Cargo.toml
├─ rust-toolchain.toml
├─ .sqlx/                              # sqlx prepare 산출 (커밋)
├─ migrations/
│   └─ 20260519120000_0001_init.sql
└─ src/
   ├─ main.rs                          # clap entry, subcommand dispatch
   ├─ cli.rs                           # subcommand structs
   ├─ telemetry.rs                     # tracing subscriber init
   ├─ error.rs                         # WitmccError (thiserror) + Result alias
   ├─ ids.rs                           # new_event_id (ULID), derive_node_id, derive_edge_id
   ├─ paths.rs                         # ~/.claude/projects 해석, JSONL 후보 산출
   ├─ ingest/
   │   ├─ mod.rs                       # ingest_path orchestrator
   │   ├─ discovery.rs                 # walkdir + extension filter
   │   ├─ transcript.rs                # JSONL streaming parser + ParsedRecord enum
   │   ├─ mapping.rs                   # ParsedRecord → ObservedEvent (mapping table 구현)
   │   └─ store.rs                     # raw_event + observed_event insert + turn_id backfill
   ├─ model/                           # 순수 타입, I/O 의존성 없음
   │   ├─ mod.rs
   │   ├─ raw.rs                       # RawEvent
   │   ├─ observed.rs                  # ObservedEvent, Actor, EventKind, CorrelationKeys
   │   ├─ graph.rs                     # GraphNode, GraphEdge, EdgeKind
   │   └─ meta.rs                      # ResponseMeta envelope, SchemaVersion 상수
   ├─ db/
   │   ├─ mod.rs                       # SqlitePool 생성 (WAL/FK/busy_timeout)
   │   ├─ repo_raw.rs
   │   ├─ repo_observed.rs
   │   ├─ repo_graph.rs
   │   └─ repo_runs.rs                 # ingest_run 기록
   ├─ graph/
   │   └─ build.rs                     # rebuild_session: deterministic node/edge 생성
   └─ api/
       ├─ mod.rs                       # Router + bind
       ├─ middleware.rs                # Host 검증, tracing
       ├─ routes.rs                    # 4 GET handlers
       └─ dto.rs                       # 응답 DTO + envelope
```

## Data Flow

```
~/.claude/projects/**/<uuid>.jsonl
        │
        ▼   ingest::discovery        (walkdir, *.jsonl)
   list of file paths
        │
        ▼   ingest::transcript       (tokio LinesStream, 64 KB BufReader, MAX_LINE=4 MiB)
   (LineMeta { path, line_no, byte_offset }, ParsedRecord)
        │
        ▼   ingest::mapping          (ParsedRecord → ObservedEvent + payload)
        │
        ▼   ingest::store            (single txn per file)
        │       ├─ INSERT INTO raw_event (ON CONFLICT DO NOTHING)
        │       └─ INSERT INTO observed_event
        │
        ▼   ingest::store::backfill_turn_ids   (per session)
        │       UPDATE observed_event SET turn_id = (transitive promptId)
        │
        ▼   graph::build::rebuild_session      (impacted session set)
        │       DELETE graph_node/graph_edge WHERE session_id=?
        │       SELECT observed_event ORDER BY observed_at, source_line_no
        │       compute nodes (hash-derived id) + 3 deterministic edge kinds
        │       INSERT graph_node, graph_edge
        │
        ▼   serve (별도 호출)
        │       axum router → repo_observed/repo_graph → DTO + envelope → JSON
```

## SQLite Schema (`migrations/20260519120000_0001_init.sql`)

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE ingest_run (
    run_id          TEXT PRIMARY KEY,                  -- ULID
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    status          TEXT NOT NULL,                     -- 'running'|'ok'|'failed'
    stats           TEXT                               -- JSON
);

CREATE TABLE raw_event (
    raw_event_id        TEXT PRIMARY KEY,              -- ULID
    ingest_run_id       TEXT NOT NULL REFERENCES ingest_run(run_id),
    source_type         TEXT NOT NULL,                 -- 'claude_transcript' | 'unparseable'
    source_uri          TEXT NOT NULL,                 -- absolute file path
    source_line_no      INTEGER NOT NULL,              -- 1-based
    source_byte_offset  INTEGER NOT NULL,              -- start of line
    payload_sha256      TEXT NOT NULL,                 -- hex(sha256(raw_line_bytes))
    payload             BLOB NOT NULL,                 -- raw line bytes
    parse_error         TEXT,                          -- non-null only when source_type='unparseable'
    captured_at         TEXT NOT NULL,
    UNIQUE(source_uri, source_line_no, payload_sha256)
);
CREATE INDEX idx_raw_event_source ON raw_event(source_uri, source_line_no);

CREATE TABLE observed_event (
    event_id                  TEXT PRIMARY KEY,        -- ULID, time-sortable
    raw_event_id              TEXT NOT NULL REFERENCES raw_event(raw_event_id),
    schema_version            TEXT NOT NULL,           -- '0.1.0'
    session_id                TEXT NOT NULL,
    event_uuid                TEXT,                    -- envelope.uuid
    parent_uuid               TEXT,                    -- envelope.parentUuid
    observed_at               TEXT NOT NULL,           -- RFC-3339 UTC
    actor                     TEXT NOT NULL,           -- 'user'|'assistant'|'system'|'hook'|'tool'
    kind                      TEXT NOT NULL,           -- §Event Kind taxonomy
    subkind                   TEXT,                    -- e.g. 'permission_mode', 'stop_hook_summary'
    tool_use_id               TEXT,
    tool_name                 TEXT,
    request_id                TEXT,                    -- assistant.requestId
    message_id                TEXT,                    -- message.id (msg_xxx)
    turn_id                   TEXT,                    -- backfilled from promptId chain
    source_tool_assistant_uuid TEXT,                   -- user.sourceToolAssistantUUID
    source_tool_use_id        TEXT,                    -- user.sourceToolUseID
    is_sidechain              INTEGER NOT NULL DEFAULT 0,
    is_meta                   INTEGER NOT NULL DEFAULT 0,
    cwd                       TEXT,
    git_branch                TEXT,
    user_type                 TEXT,
    entrypoint                TEXT,
    cc_version                TEXT,                    -- envelope.version
    payload                   TEXT NOT NULL,           -- JSON: original inner body
    -- OTel facet — populated by future collectors, NULL here
    trace_id                  TEXT,
    span_id                   TEXT,
    parent_span_id            TEXT,
    latency_ms                INTEGER,
    -- redaction facet — populated by future redaction gate, NULL here
    redaction_state           TEXT,                    -- NULL | 'clean' | 'redacted'
    parser_version            TEXT NOT NULL            -- e.g. 'transcript@0.1.0'
);
CREATE INDEX idx_obs_session_time   ON observed_event(session_id, observed_at);
CREATE INDEX idx_obs_tool_use_id    ON observed_event(tool_use_id) WHERE tool_use_id IS NOT NULL;
CREATE INDEX idx_obs_event_uuid     ON observed_event(event_uuid) WHERE event_uuid IS NOT NULL;
CREATE INDEX idx_obs_parent_uuid    ON observed_event(parent_uuid) WHERE parent_uuid IS NOT NULL;
CREATE INDEX idx_obs_turn_id        ON observed_event(session_id, turn_id);

CREATE TABLE graph_node (
    node_id             TEXT PRIMARY KEY,              -- hash-derived
    schema_version      TEXT NOT NULL,
    session_id          TEXT NOT NULL,
    node_kind           TEXT NOT NULL,                 -- §Node Kind taxonomy
    started_at          TEXT NOT NULL,
    ended_at            TEXT,
    merge_keys          TEXT NOT NULL,                 -- JSON, sorted-key canonical
    source_event_ids    TEXT NOT NULL,                 -- JSON array of ULID
    source_uris         TEXT NOT NULL,                 -- JSON array
    payload             TEXT NOT NULL                  -- JSON
);
CREATE INDEX idx_graph_node_session ON graph_node(session_id, started_at);
CREATE INDEX idx_graph_node_kind    ON graph_node(session_id, node_kind);

CREATE TABLE graph_edge (
    edge_id          TEXT PRIMARY KEY,                 -- hash-derived
    schema_version   TEXT NOT NULL,
    session_id       TEXT NOT NULL,
    from_node_id     TEXT NOT NULL REFERENCES graph_node(node_id),
    to_node_id       TEXT NOT NULL REFERENCES graph_node(node_id),
    edge_kind        TEXT NOT NULL,                    -- 'turn_order'|'tool_call_to_result'|'message_reply'
    origin           TEXT NOT NULL DEFAULT 'deterministic',
    attributes       TEXT NOT NULL DEFAULT '{}'        -- JSON
);
CREATE INDEX idx_graph_edge_session ON graph_edge(session_id, edge_kind);
CREATE INDEX idx_graph_edge_from    ON graph_edge(from_node_id);
CREATE INDEX idx_graph_edge_to      ON graph_edge(to_node_id);
```

DB 초기화 시 `init-db` 서브커맨드가 추가로:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

## ObservedEvent Mapping (Transcript JSONL → ObservedEvent)

실데이터(`~/.claude/projects/-Users-bahamoth-projects-whats-in-my-cc/3a07124f-….jsonl`, 200 라인)에서 관측된 type 분포: `attachment×81, assistant×62, user×36, last-prompt×11, permission-mode×11, system×2, file-history-snapshot×1`.

### 공통 envelope (`assistant` / `user` / `attachment` / `system`)
`uuid, parentUuid, sessionId, timestamp, cwd, gitBranch, entrypoint, userType, version, isSidechain`.

### Event Kind taxonomy (slice-1 고정)

```
kind ∈ {
  user_message, assistant_message, thinking, tool_call, tool_result,
  hook_event, system_summary, session_state, file_history_snapshot,
  attachment_meta, unknown
}
```

### Per-type mapping

| 원천 type (+ subtype) | actor | kind | correlation_keys 추출 | payload(JSON)에만 보존 |
|---|---|---|---|---|
| `user` (string content) | user | user_message | `turn_id=promptId` | text |
| `user` (array text) | user | user_message | `turn_id=promptId` | content[] |
| `user` (array, tool_result) | system | tool_result | `tool_use_id=content[].tool_use_id`, `turn_id=promptId`, `source_tool_assistant_uuid` | `is_error`, content |
| `user` (isMeta=1) | system | user_message (is_meta=1) | `turn_id=promptId`, opt. `source_tool_use_id` | content |
| `assistant` content[type=text] | assistant | assistant_message | `request_id`, `message_id` | text |
| `assistant` content[type=thinking] | assistant | thinking | `request_id`, `message_id` | signature, blob |
| `assistant` content[type=tool_use] | assistant | tool_call | `tool_use_id=content[].id`, `tool_name=content[].name`, `request_id`, `message_id` | `input` |
| `attachment` (`hook_success`, `hook_additional_context`) | hook | hook_event | `tool_use_id=attachment.toolUseID`, `subkind=attachment.hookEvent`, `tool_name=attachment.hookName` | `command, stdout, stderr, exitCode, durationMs, content` |
| `attachment` (other subtypes) | system | attachment_meta | none | full attachment object |
| `system` (subtype=stop_hook_summary) | system | system_summary | `tool_use_id=toolUseID`, `subkind='stop_hook_summary'` | hookInfos, hookErrors, stopReason |
| `permission-mode` | system | session_state | `subkind='permission_mode'`, `session_id` only | permissionMode |
| `last-prompt` | system | session_state | `subkind='last_prompt'`, `session_id`, `event_uuid=leafUuid` | — |
| `file-history-snapshot` | system | file_history_snapshot | `message_id` | snapshot dict |
| (anything else) | system | unknown | none | raw inner |

### Splitting `assistant` content into multiple ObservedEvents

한 `assistant` JSONL 라인은 `content[]`에 text/thinking/tool_use 혼합이 가능하다. slice-1은 **content 요소별로 별도 ObservedEvent**를 만든다(같은 `raw_event_id`를 공유, 다른 `event_id`). content 내 ordinal을 `payload.content_ordinal`로 보존.

### Turn ID backfill

`promptId`는 `user`에만 등장. ingest 종료 시점에 세션 단위로:
```
parent_index: Map<event_uuid, parent_uuid>
prompt_index: Map<event_uuid, promptId>           -- user 이벤트에만
for each event without turn_id:
  walk parent chain → first promptId hit → set turn_id
  (no hit → leave NULL)
```
이는 단일 UPDATE 또는 in-memory 계산 후 batch UPDATE.

## Deterministic Graph Builder

`rebuild_session(session_id)`는 세션 단위 멱등. 알고리즘:

### Node 식별 (hash-derived id)

```
node_id  = "nd_" || hex(sha256(node_kind || "|" || canonical(merge_keys)))[..24]
edge_id  = "eg_" || hex(sha256(from_node_id || ">" || to_node_id || "#" || edge_kind))[..24]
```
`canonical(merge_keys)`: key 사전순 정렬, `k1=v1;k2=v2;…`.

### Node materialization rules

| node_kind | 만드는 ObservedEvent | merge_keys | 합치는 다른 event |
|---|---|---|---|
| `user_message` | user_message | `{session_id, event_uuid}` | — |
| `assistant_message` | assistant_message (text) | `{session_id, event_uuid}` | 같은 라인의 thinking을 payload에 포함, `has_thinking=true` |
| `tool_call` | assistant tool_call | `{session_id, tool_use_id}` | matching tool_result이 있으면 payload에 `result`로 포함 |
| `tool_result` (dangling) | tool_result with no matching call | `{session_id, tool_use_id}` | — |
| `hook_event` | attachment(hook_success / hook_additional_context) | `{session_id, hook_event_uuid}` | — |

`attachment_meta`, `session_state`, `file_history_snapshot`, `thinking` 단독은 노드를 만들지 않는다 (payload 보존됨, 후속 spec에서 분리 가능).

### Edge kinds (slice-1 3종)

1. **`turn_order`** — 한 세션 내 node를 `(started_at ASC, source_line_no ASC)`로 정렬한 인접쌍에 edge.  attributes: `{}`.
2. **`tool_call_to_result`** — 동일 `tool_use_id`로 tool_call → 매칭되는 tool_result 노드. 매칭은 (a) tool_call payload에 result가 합쳐진 경우는 edge 생략(이미 같은 노드), (b) dangling tool_result만 별도 노드일 때 edge 생성. attributes: `{matched_via: "tool_use_id"}`.
3. **`message_reply`** — `parent_uuid → node` 매핑으로 부모 노드 → 자식 노드. sidechain 경계 cross 시 `attributes.crosses_sidechain=true`.

### 결정성 보장

- 모든 노드/엣지 id가 입력에 의존하는 결정적 해시.
- ULID는 `raw_event`/`observed_event`/`ingest_run` PK에만 사용 — 그래프 결과에 영향 없음.
- 정렬은 `(observed_at, source_line_no)` 보조키로 동률 해소.
- payload_sha256은 **원본 라인 바이트**(파싱 전) 기준 → JSON canonical 비용 없음.
- 같은 fixture에 두 번 ingest해도 raw_event/observed_event/graph_node/graph_edge 행 수와 PK 집합 동일.

## Idempotency

- `raw_event`의 UNIQUE `(source_uri, source_line_no, payload_sha256)` → 동일 파일·라인·내용 재삽입 거부.
- JSONL이 append-only로 자라는 경우: `ingest_run.stats`에 파일별 `last_byte_offset` 기록, 다음 실행은 `seek`로 이어 읽기.
- 파일이 잘렸다 재생성되어 같은 (path, line)인데 내용 바뀐 경우: payload_sha256 다르므로 새 row 인정.
- 그래프는 **세션 단위 DELETE+INSERT**(`rebuild_session`)로 재구성. node_id가 결정적이므로 외부 참조도 깨지지 않음.

## Pull API

127.0.0.1:7878 (`--bind` 또는 `--port`로 변경). 모든 응답:

```json
{
  "meta": {
    "schema_version": "0.1.0",
    "collection_profile": "local_transcript_slice1",
    "redaction_policy": null,
    "generated_at": "2026-05-19T03:14:15Z"
  },
  "data": …
}
```

### Endpoints

| Method · Path | data 모양 |
|---|---|
| `GET /v1/health` | `{status:"ok", build_sha:"…"}` (envelope 면제) |
| `GET /v1/sessions?limit=&cursor=` | `[{session_id, first_observed_at, last_observed_at, event_count, source_uris[]}]` |
| `GET /v1/sessions/{id}?limit=&cursor=` | `{session_id, summary:{event_count, by_kind, first_observed_at, last_observed_at}, events:[ObservedEventDTO]}` |
| `GET /v1/sessions/{id}/graph` | `{nodes:[GraphNodeDTO], edges:[GraphEdgeDTO]}` |

### Pagination

slice-1은 `?limit=` (default 500, max 5000)만 지원. `cursor`는 응답 envelope에 `next_cursor=null`로 자리만 잡고 미구현 표시.

### Errors

`application/problem+json`(RFC 7807). slice-1에서 정의:
- `404 RESOURCE_NOT_FOUND` — 세션 미존재
- `400 BAD_REQUEST` — limit 범위 위반 등
- `500 INTERNAL` — 핸들러 panic 캐치 미들웨어

### Middleware (slice-1 보안 기본선)

- bind는 127.0.0.1 강제. 명령행에서 다른 IP 지정 시 startup 거부.
- `Host` 헤더 화이트리스트(`127.0.0.1:<port>`, `localhost:<port>`) — DNS rebinding 1차 방어.
- CORS: deny-all (no `Access-Control-Allow-Origin`).
- `tower_http::trace::TraceLayer`.

## Error Handling

| 상황 | 동작 |
|---|---|
| JSON parse 실패 | raw_event(source_type=`unparseable`, parse_error=msg) 저장, ObservedEvent 미생성, `warn!` 1회 |
| 필수 필드 누락 (`type` 없음) | 위와 동일 |
| 알 수 없는 `type` | raw_event 정상, ObservedEvent `kind='unknown'` + 전체 payload 보존 |
| 파일 IO 실패 | 해당 파일만 skip, ingest_run.stats.failed_files에 기록 |
| `MAX_LINE_BYTES=4 MiB` 초과 | truncate + parse_error=`line too large`, raw_event 저장은 시도 (BLOB truncated) |
| envelope.sessionId 부재 | path basename에서 session_id 추론, envelope 값과 불일치 시 `warn!` |
| handler panic | tower catch 미들웨어 → 500 problem+json |
| migrate pending on serve | default는 startup 거부(코드 5 exit). `--auto-migrate` 플래그 있으면 자동 적용 후 계속 |

ingest는 **부분 실패가 전체 실패를 막지 않음**(file 단위 격리).

## CLI

```
witmcc init-db [--db-path PATH]
witmcc ingest  [--db-path PATH] [--path DIR] [--all]
                # --all: ~/.claude/projects/**/*.jsonl 자동 발견
                # --path: 특정 디렉터리/파일만
witmcc serve   [--db-path PATH] [--bind 127.0.0.1] [--port 7878]
```

공통 옵션:
- `--db-path` (default `./.witmcc.sqlite`)
- `--log-format pretty|json` (default pretty)
- `--verbose` (=`RUST_LOG=debug` 강제)

## Testing Strategy

### Fixtures
`tests/fixtures/transcripts/` 아래:
- `minimal_session.jsonl` — 5라인, 정상 흐름, 모든 edge 종류 검증
- `dangling_tool_use.jsonl` — tool_use 후 tool_result 누락
- `sidechain.jsonl` — isSidechain=true 가지가 main에 붙음
- `malformed_lines.jsonl` — JSON 파싱 실패 + 알 수 없는 type 섞임
- `large_thinking.jsonl` — 1 MB thinking blob 1라인
- `golden_session.jsonl` — 실제 transcript의 sanitize 사본 (서명/토큰 마스킹)

### 단언

- 단위(parser): 각 distinct type/subtype에 대해 `parse_line` 결과 검증
- 단위(graph): `insta::assert_json_snapshot!`로 회귀, ULID는 redaction 마스크
- DB 통합: in-memory `sqlite::memory:` migrate 후 ingest 2회 → row 수·PK 동일(멱등)
- API 통합: `axum-test::TestServer` + `assert-json-diff::assert_json_include` 4개 엔드포인트
- 결정성: 같은 fixture 두 번 빌드 → node_id/edge_id 집합 동일(`pretty_assertions`)
- CLI smoke: `assert_cmd` + `tempfile` — init-db → ingest → serve(0번 포트) → health 검증

### CI
- `cargo fmt --check`, `cargo clippy --all-targets --deny warnings`, `cargo test`.
- `SQLX_OFFLINE=true`로 빌드 (`.sqlx/` 디렉터리 커밋).

## Crate List

```toml
[dependencies]
tokio        = { version = "1.40", features = ["macros","rt-multi-thread","fs","io-util","signal"] }
axum         = { version = "0.7",  features = ["macros","json","tokio"] }
tower        = "0.5"
tower-http   = { version = "0.5",  features = ["trace"] }
sqlx         = { version = "0.8",  default-features = false, features = ["runtime-tokio-rustls","sqlite","macros","migrate","json","chrono"] }
serde        = { version = "1",    features = ["derive"] }
serde_json   = { version = "1",    features = ["raw_value","preserve_order"] }
clap         = { version = "4.5",  features = ["derive","env"] }
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter","json"] }
ulid         = { version = "1.1",  features = ["serde"] }
chrono       = { version = "0.4",  features = ["serde"] }
anyhow       = "1"
thiserror    = "2"
dirs         = "5"
walkdir      = "2"
futures      = "0.3"
sha2         = "0.10"
hex          = "0.4"
once_cell    = "1"

[dev-dependencies]
assert-json-diff  = "2"
insta             = { version = "1", features = ["json","redactions"] }
tempfile          = "3"
http-body-util    = "0.1"
axum-test         = "16"
pretty_assertions = "1"
assert_cmd        = "2"
```

## Forward-Compatibility Locks (후속 spec과의 계약)

이후 spec이 깨면 안 되는 약속:
- `raw_event` 테이블은 절대 변경 없이 ALTER ADD만(SQLite 제약상 NOT NULL 신규 컬럼은 DEFAULT와 함께만).
- `observed_event.schema_version` / `parser_version` / `redaction_state` / OTel 컬럼 4개는 이미 존재 — 후속에서 채우기만.
- API envelope `meta`는 추가 필드만(`redaction_policy`, `collection_profile`을 nullable로 미리 보냄).
- module 디렉토리 구조 — `model`은 다른 모듈에 의존하지 않음(순수 타입).
- 외부 listen은 127.0.0.1 only — 후속에서 옵션화하더라도 default 유지.
- POST·PATCH 외부 write는 영구 금지(PRD non-goal).

## Implementation Step List (writing-plans 입력용)

| # | Step | Size | 핵심 산출 |
|---|---|---|---|
| 1 | 프로젝트 부트스트랩 (`cargo init`, toolchain, Cargo.toml, clap 스켈레톤) | S 0.5d | `cargo run -- --help` 동작 |
| 2 | telemetry / error / ids 모듈 + 결정성 단위 테스트 | S 0.5d | derive_node_id 결정성 테스트 GREEN |
| 3 | DB 모듈 + `migrations/…_0001_init.sql` + `init-db` 서브커맨드 + WAL 설정 | M 1d | in-memory migrate 테스트 GREEN |
| 4 | JSONL 스트리밍 파서(`ingest::transcript`) + ParsedRecord enum + per-type fixtures | M 1d | 7 type 모두 parse 단위 테스트 |
| 5 | ObservedEvent mapping(`ingest::mapping`) + content split + raw/observed insert | M 1d | `ingest --path fixture/` → 두 테이블 row 채워짐, 멱등 |
| 6 | Turn ID backfill (`ingest::store::backfill_turn_ids`) | S 0.5d | promptId 사슬 단위 테스트 |
| 7 | 그래프 빌더(`graph::build::rebuild_session`) + insta snapshot | M 1d | minimal_session에서 모든 edge 종류 검증 |
| 8 | API 레이어(`api::*`) + 4개 endpoint + Host 미들웨어 | M 1d | axum-test 4개 GREEN |
| 9 | CLI 마무리(`--all` 자동 발견, `--port`), `assert_cmd` smoke | S 0.5d | end-to-end CLI 통과 |
| 10 | 결정성 회귀 + golden_session sanitize | S 0.5d | 같은 fixture 2회 빌드 동일 PK 집합 |
| 11 | README + `docs/implementation-notes.html` 갱신 | S 0.5d | 사용 예시, 알려진 한계 명시 |

총 추정: 1인 7–8 일.

## Open Questions (slice-1 범위 외, 후속 spec 입력)

1. raw API body(claude API 원본 응답)를 `local_full_evidence` 기본에 포함할지 — PRD §09 미해결.
2. MCP endpoint 접근 범위 — 같은 머신만인지 LAN 허용인지(PRD §09).
3. export bundle에 raw payload 포함 여부(PRD §09).
4. `attachment` 비-hook subtype을 향후 어디까지 노드화할지 — slice-1은 payload-only.
5. resumed/forked session(동일 sessionId의 multi-file)이 실제로 관측되는가 — slice-1은 합쳐 처리하되 모니터링 필요.
