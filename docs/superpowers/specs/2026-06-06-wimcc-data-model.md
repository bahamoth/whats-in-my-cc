# wimcc 전체 데이터 모델 (2026-06-06 코드 기준)

> Insight per-kind 재설계를 판단하기 위한 전체 맥락. **source of truth = 실제 코드·migration**
> (HTML 사양서 00~04는 event-first 재설계 이후 일부 낡음). 모든 file:line은 2026-06-06 main+`feat/insight-per-kind-blocks` 기준.

> **⚠ 이 문서는 삭제 이전 상태의 분석(근거 문서)이다.** 여기서 식별한 judge·graph 비효율은
> PR #37 (`refactor/drop-judge-graph`)에서 **삭제로 해결**됐다. 아래의 judge/graph 서술은
> "왜 삭제했는가"의 근거이며 현재 코드에는 존재하지 않는다. 현재 상태:
> `docs/implementation-notes.html#judge-removal` · `#graph-removal` · `#span-dedup`,
> 실행 계획: `docs/superpowers/plans/2026-06-06-drop-judge-graph-layers.md`.

---

## 0. 한 장 요약 — 파이프라인

```
 RAW SOURCES                STAGE 1            STAGE 2                STAGE 3 (transcript only)     STAGE 4
 ───────────                ───────            ───────                ─────────────────────         ───────
 transcript JSONL ─┐                                                  diff_hunk    (side table)
 OTLP spans       ─┤        raw_event   ──►    observed_event  ──┬──► verification_run (side)  ──►  graph rebuild
 OTLP logs        ─┤  (verbatim bytes,   normalize+correlate    │    usage_facet  (side table)      (graph_node
 OTLP metrics     ─┤   sha256 dedup,     → 14 EventKind         │    turn_id backfill                + graph_edge,
 hook stdin       ─┤   redaction)        + correlation keys     │                                     deterministic
 git2 diff(트랜스)─┘                      + telemetry facet      └──► insight pipeline ──► finding     + inferred)
                                                                       (L1 extractors → judge L2)

                         ┌──────────────────── read-only Pull API (/v1/*) + MCP (/mcp) + SSE ──────────────────────┐
 VIEWS (WebUI)           │  events window · raw · findings · graph · usage · diff-hunks · verification-runs        │
 ───────────             │                                                                                          │
 message / detail / raw  ── ObservedEvent + correlation 키로 직접 그림 (graph backing 아님; event-first PR #33)
 graph view / MCP        ── graph_node/graph_edge (causal edge inference 전용)
```

- 정규화의 중심은 **`ObservedEvent` 단일 타입**. 모든 소스가 여기로 수렴(diff/verification/usage 제외 — 별도 테이블).
- 그래프는 **부산물**: 매 ingest 끝에 세션별 rebuild되지만 뷰의 backing model이 아니다.
- 외부 노출은 **read-only**. write/correction/label/status 엔드포인트 없음(확인됨).

---

## 1. 저장 스키마 — 14개 테이블 (migration 0001~0017, episode는 0017에서 drop)

```
ingest_run ──1:N──► raw_event ──1:N──► observed_event ──┬─1:N─► diff_hunk         (introduced_by_event_id)
   (job)            (verbatim,         (정규화 fact,     ├─1:N─► verification_run  (trigger_event_id)
                     sha256 dedup,      payload=JSON)    └─(raw)─► usage_facet      (raw_event_id PK·1:1 turn)
                     redaction)
                                        observed_event ──► graph_node ──N:M(edge)──► graph_node
                                                            (rebuild)     graph_edge

 finding ◄─evidence_refs(event_id[])── observed_event      findings_pending_judge ──► finding (judge 통과 시)
   │                                                        judge_verdict_cache (cross-session, 30d retention)
 housekeeping: retention_tombstone (410 Gone), audit (보안/운영 로그)
```

| 테이블 | 역할 | 핵심 컬럼 / JSON BLOB | migration |
|---|---|---|---|
| `ingest_run` | ingest job 생명주기 | `stats`(BLOB) | 0001 |
| `raw_event` | **source 보존**(verbatim) | `payload`(BLOB), `payload_sha256`, `source_uri/line/offset`, `redaction_state/manifest` | 0001, 0011 |
| `observed_event` | **정규화 fact 테이블** | `payload`(JSON BLOB) + 아래 §2 컬럼 전부 | 0001, 0002, 0004 |
| `graph_node` | causal 노드 | `merge_keys`, `source_event_ids`(JSON[]), `payload`(BLOB) | 0001 |
| `graph_edge` | causal 엣지 | `origin`(deterministic\|inferred), `inference_rule_id`, `confidence` | 0001, 0007 |
| `diff_hunk` | 파일 변경(side) | `file_path`, `patch_preview`, `introduced_by_event_id`(FK), `user_modified` | 0003 |
| `verification_run` | test/build/lint(side) | `command_kind`, `status`, `trigger_event_id`(FK), `detection_basis`, `status_basis` | 0005, 0015 |
| `usage_facet` | 토큰 사용(side, 1:1 turn) | `input/output/cache_* tokens`, `model`, PK=`raw_event_id` | 0014 |
| `finding` | **evidence-linked insight** | `category`, `subkind`, `severity`, `confidence`, `evidence_refs`(JSON[]), `evidence_projection`(BLOB), `provenance`(BLOB), `status` | 0008, 0016 |
| `findings_pending_judge` | judge 대기 큐(절대 silent drop 안 함) | `confidence_l1`, `attempts` | 0010 |
| `judge_verdict_cache` | LLM 판정 캐시(cross-session) | `verdict_json`(BLOB), `prompt_template_version` | 0009 |
| `retention_tombstone` | 만료 삭제 기록(404 vs 410 구분) | `resource_kind`, `reason` | 0012 |
| `audit` | 보안/운영 로그 | `event`, `payload`(BLOB) | 0013 |
| ~~`episode`~~ | (폐기) phase 분류 | — | 0006 생성 → **0017 drop** |

핵심 인덱스(상관 키 기반): `idx_obs_session_time(session_id,observed_at)`,
`idx_obs_tool_use_id`, `idx_obs_event_uuid`, `idx_obs_parent_uuid`, `idx_obs_turn_id(session_id,turn_id)`,
`idx_obs_trace_span(trace_id,span_id)`, 그리고 payload JSON 인덱스
`json_extract(payload,'$.instrument_name')`(metric), `json_extract(payload,'$.event_name')`(log).

---

## 2. `ObservedEvent` — 중심 엔티티 (`src/model/observed.rs:135-167`)

Insight는 **선택된 ObservedEvent 1건**에 대한 뷰이므로 이 엔티티가 설계의 핵심.

```
ObservedEvent
├─ identity/envelope (항상)
│   event_id, raw_event_id(→raw_event FK), schema_version="0.5.0", session_id, observed_at, actor
├─ classification
│   kind(enum, snake_case), subkind(opt), tool_name(opt)
├─ correlation keys (전부 opt — OTel-first, 1급 필드)
│   transcript: event_uuid, parent_uuid, turn_id, message_id, request_id
│   tool:       tool_use_id
│   OTel:       trace_id, span_id, parent_span_id
│   tool-result lineage: source_tool_assistant_uuid, source_tool_use_id   (wire에 미노출)
├─ telemetry facet (opt; kind=otel_span일 때) + latency_ms
├─ context: cwd, git_branch, user_type, entrypoint, cc_version              (wire에 미노출)
├─ flags: is_sidechain, is_meta
├─ payload: serde_json::Value  (kind별 내용 — §3)
└─ provenance: parser_version  ("transcript@0.1.0" | "otel@0.1.0" | "otel-metrics@0.5" | "otel-logs@0.5" | "hook@0.1.0" | "file_git@0.1.0")
```

### 2.1 wire DTO vs 내부 vs 프론트 TS — Insight에 중요

`observed_to_dto()`(`src/api/routes.rs:629-657`)가 wire로 내보내는 것:

| 그룹 | 필드 | wire DTO | 프론트 `types.ts` |
|---|---|---|---|
| 식별/분류 | event_id, raw_event_id, session_id, observed_at, actor, kind, subkind, tool_name | ✓ | ✓ |
| 상관 | event_uuid, parent_uuid, turn_id, tool_use_id | ✓ | ✓ |
| 상관 | request_id, message_id | ✓ | ✓(opt) |
| **OTel** | **trace_id, span_id, parent_span_id, latency_ms** | **✓** | **✗ 누락** |
| **telemetry facet** | telemetry{span_name,kind,status,attrs,resource,...} | **✓** | ✓(부분) |
| payload | payload | ✓ | ✓ |
| 내부전용(미노출) | cwd, git_branch, cc_version, parser_version, source_tool_* | ✗ | ✗ |

> **⚠ Insight spec 정정점:** `trace_id/span_id/latency_ms/telemetry`는 **events 목록 wire DTO에
> 이미 존재**한다. raw 쿼리(`RawEventResponse.telemetry`)가 *유일한* 출처가 아니다 —
> 프론트 `ObservedEventDto` **TS 타입만 확장**하면 공통 telemetry/correlation 레이어를
> events 목록에서 바로 채울 수 있다. (Insight 재설계 spec §4.3의 "DTO엔 없다" 서술은 TS 타입
> 한정이며, wire에는 있음.)

### 2.2 kind 분류 (`EventKind` enum, 14 variant)

`user_message · assistant_message · thinking · tool_call · tool_result · hook_event ·
system_summary · session_state · attachment_meta · otel_span · diff_hunk · metric_sample ·
log_record · unknown`

`subkind`은 opt string(전용 enum 없음). hook은 `subkind`에 hook_event_name을 넣음.
**실제 카드 구성의 판별자는 kind보다 잘다**(Insight spec §4.1과 동일 결론):
`log_record→payload.event_name`, `attachment_meta→payload.type`, `tool_*→tool_name`.

### 2.3 facet (payload 안에 직렬화)

- **TelemetryFacet**(`telemetry` 필드, otel_span): span_name, span_kind, status_code/message,
  start/end_unix_nano, attributes, resource, scope_name/version.
- **MetricFacet**(payload, metric_sample): instrument_name/kind, unit, value_int/value_float,
  temporality, is_monotonic, histogram(원본 보존), attributes, resource, time_unix_nano.
- **LogFacet**(payload, log_record): severity_number/text, body(원본), event_name(인덱싱용),
  attributes, resource, time_unix_nano.

---

## 3. 소스 → kind 매핑 & 상관 (`src/ingest/`)

| 소스 | parser | 생성 kind | 부여되는 상관 키 |
|---|---|---|---|
| transcript JSONL | `transcript@0.1.0` | user_message, assistant_message, thinking, tool_call, tool_result, hook_event, attachment_meta, system_summary, session_state | event_uuid, parent_uuid, turn_id(backfill), message_id, request_id, tool_use_id |
| OTLP spans | `otel@0.1.0` | otel_span | trace_id, span_id, parent_span_id, session_id(attr) |
| OTLP logs | `otel-logs@0.5` | log_record | trace_id, span_id, session_id(attr) |
| OTLP metrics | `otel-metrics@0.5` | metric_sample | session_id(attr) — **trace/span 없음** |
| hook stdin | `hook@0.1.0` | hook_event | tool_use_id, tool_name, subkind |
| git diff(트랜스 내 structuredPatch) | `file_git@0.1.0` | (diff_hunk **테이블**, ObservedEvent 아님) | introduced_by_event_id, tool_use_id |

### 3.1 상관(correlation)의 실제 — Insight 공통 레이어 설계에 직결

```
                          request_id ───────────────┐
 assistant_message ──┬── message_id                 │  (transcript ↔ OTel llm_request span)
                     │                               ▼
 tool_call ──tool_use_id──► tool_result        otel_span(llm_request) ·· api_request log
     │                          │                    ▲
     └── hook_event ───tool_use_id                   │
                                            log_record / metric_sample (session_id, 일부 request_id/tool_use_id attr)

 turn_id ── 한 프롬프트의 모든 이벤트 묶음(parent_uuid 체인으로 backfill)
 event_uuid/parent_uuid ── transcript lineage
```

**중요한 한계(코드 확인):**
- OTel↔transcript **자동 cross-link 없음**. span은 `tool_use_id`를 안 가짐 → 공유 키
  (`session_id`, `request_id`(llm_request), 일부 attr)로만 상관. 프론트
  `buildRawBlocksFromEvents`가 하는 매칭이 사실상 유일한 상관 경로.
- metric_sample엔 trace/span 없음 → span 단위 상관 불가, session/instrument 단위만.
- diff/verification/usage는 ObservedEvent가 아니라 **별도 API**(`/diff-hunks`, `/verification-runs`, `/usage`)로 노출.

**source-preserving 확인:** 모든 ObservedEvent는 `raw_event_id`로 verbatim 원본을 가리킴.
단 diff_hunk/verification_run은 완전 정규화되어 raw 참조를 따로 안 들고 있음(예외).

---

## 4. 그래프 — 역할 한정 (`src/model/graph.rs`, `src/graph/build.rs`)

매 ingest Stage 4에서 세션별 `rebuild_session`. **뷰 backing 아님**(event-first PR #33).
오직 (a) causal edge inference, (b) `/graph` Pull API + MCP `get_session_graph`/`explain_node`.

- **GraphNode**: node_id, node_kind, started_at, merge_keys, `source_event_ids`(어느 ObservedEvent들이 이 노드를 구성했나), payload. 노드로 승격되는 kind는 일부(attachment_meta/session_state/thinking/system_summary/unknown은 **노드 안 됨**).
- **GraphEdge**: from/to, edge_kind, `origin`(deterministic|inferred), `inference_rule_id`, `confidence`.
  - deterministic: `message_reply`, `tool_call_to_result`(tool_use_id 쌍).
  - inferred(rule, 전부 versioned):
    - `caused_repair@v1` — error tool_call → 60s내 토큰 2개+ 공유하는 다음 tool_call. conf=0.7·overlap+0.3·time_decay.
    - `triggered_by_user_message@v1` — user_message → (assistant 개입 없이) 다음 tool_call. conf=0.85.
    - `large_output_to_next_action@v1` — payload≥50KB tool_call → 다음 assistant_message. conf=0.6+0.4·norm(size).

---

## 5. Findings + Judge — evidence-linked insight (`src/insight/`)

`RootCauseHypothesis`/`QualitySummary`는 **없음**(사양서 낡음). 단일 flat `FindingRow`:

```
finding { finding_id(=hash(category,session,evidence_refs)), category, subkind?, severity,
          confidence, summary, evidence_refs[event_id…](≥1 필수), evidence_projection(JSON),
          provenance{extractor,layer:L1|L2,judge,…}, status(active|pending_judge|discarded) }
```

- **evidence_refs 없는 finding은 만들지 않음**(AC-4, 코드 강제) — 프로젝트 "evidence-linked" 원칙.
- **L1 extractor 4종**: `tool_failure`(error tool_result, 5이벤트 내 재시도 없음; subkind=user_visible|internal_retry|benign_nonzero_exit), `risky_action`(파괴적 Bash/user-modified diff), `context_bloat`(50KB+ 결과 미재사용), `final_state_mismatch`(목표 동사 vs 완료 마커 vs verification 실패).
- **L2 judge**(opt): IfAbove(t)/Never 정책 → 통과 시 finding 승격. provider: Noop(기본)/Fixture/Anthropic. `judge_verdict_cache`로 cross-session 재사용, 예산 소진 시 `findings_pending_judge`에 남김(monotonic).

Insight 탭의 Findings 섹션은 이 finding을 `evidence_refs`로 선택 이벤트와 매칭해 보여줌(`SessionDetailPage`의 `selectedNodeFindings`).

---

## 6. API / MCP 표면 (read-only, `src/api/`)

**HTTP `/v1/*`**(bearer, default `--auth off`): `sessions`, `sessions/:id`,
`sessions/:id/events`(cursor 페이지 + 상관 telemetry), `…/graph`, `…/diff-hunks`,
`…/verification-runs`, `…/usage`, `…/findings`, `…/tool-failures`, `findings`,
`findings/:id`, `findings/:id/evidence`, `verification-runs/:id`, `usage/baseline`,
`events/:id/raw`, `audit`, `stream`(SSE).
**Ingest POST**(no auth): `/otel/v1/{traces,metrics,logs}`, `/hooks/v1/events`.
**MCP `/mcp`**: resources 6(sessions/graph/findings/finding/file-lineage/otel-trace) + tools 6
(get_session_graph, search_sessions, search_findings, explain_node, get_file_lineage, get_otel_trace).

**write 엔드포인트 없음**(확인). 응답은 `Envelope{meta{schema_version, collection_profile,
redaction_policy, redaction_summary, generated_at, next_cursor}, data}`.

---

## 7. Insight per-kind 재설계가 앉는 자리

```
선택 이벤트(ObservedEventDto, wire) ──┐
윈도우 내 상관 이벤트들 ───────────────┤──► buildInsightModel ──► InsightRenderer
findings(evidence_refs 매칭) ─────────┤        │
(필요 시) /events/:id/raw ────────────┘        ├─ identity   ← envelope (있는 것만)
                                               ├─ correlation← 상관 키 (있는 것만)
                                               ├─ telemetry  ← telemetry facet + 상관 span/metric/log  ★events DTO에 이미 있음
                                               ├─ hook       ← 상관 hook_event
                                               └─ blocks[]   ← (kind,subtype) 추출기 (티어드)
```

**이 데이터 모델이 재설계에 주는 판단 근거:**
1. Insight가 읽을 데이터는 전부 **이미 존재**한다 — events DTO(telemetry/trace/latency 포함, TS만 확장 필요) + 상관 이벤트 + findings. 새 백엔드 불필요.
2. **상관은 공유 키로만** 가능(OTel↔transcript 자동 링크 없음) → 공통 telemetry/correlation 레이어는 `buildRawBlocksFromEvents`의 키 매칭(request_id/tool_use_id/session_id)을 재사용해야 하고, 그 이상은 못 한다(정직한 한계).
3. diff/verification/usage는 ObservedEvent가 아님 → 이들은 Insight 블록이 아니라 **별도 API** 영역. tool_call/tool_result 카드에서 diff/verification을 보이려면 그 API를 추가로 끌어와야 함(이번 범위 밖, 열린 질문).
4. payload facet(metric/log/telemetry)이 이미 구조화돼 있어 → `scalar`/`keyValue`/`timeline` 블록 매핑이 자연스럽다.

---

## 부록 — 핵심 file:line

- 정규화 타입: `src/model/observed.rs:135-167`(ObservedEvent), `:70-83`(TelemetryFacet), `:88-132`(Metric/Log facet)
- raw 보존: `src/model/raw.rs:5-16`
- 그래프: `src/model/graph.rs:5-38`, `src/graph/build.rs:54-221`
- wire DTO 투영: `src/api/routes.rs:629-657`(`observed_to_dto`), `src/api/dto.rs`
- 프론트 타입(누락 확인): `webui/src/api/types.ts:13-31`(ObservedEventDto), `:234-241`(telemetry)
- ingest: `src/ingest/{transcript,otel,otel_logs,otel_metrics,hook,mapping,store}.rs`
- findings: `src/insight/{registry,pipeline}.rs`, `src/insight/extractors/*`, `src/db/repo_finding.rs:11-32`
- edge inference: `src/insight/edge_inference/rules/*`
