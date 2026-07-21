//! 단일 통합 테스트 하네스 — 파일당 1 바이너리 구조를 1 바이너리로 통합.
//!
//! 근거(2026-07-21 실측, #test-harness-merge-2026-07-21): macOS Gatekeeper가
//! 빌드마다 새로 생기는 테스트 바이너리 첫 실행에 바이너리당 ~30초의 보안
//! 검사를 걸어, 127개 바이너리 × ~30초 ≈ 1시간 정체가 발생했다(테스트 실행
//! 자체는 합계 ~20초). 통합으로 검사·링크가 각 1회가 되고, libtest가 모듈
//! 전체를 스레드 병렬로 돌린다.
//!
//! 예외 — `WIMCC_CONFIG_DIR`(프로세스 전역 env)를 변경하는 3개 파일은
//! 문서화된 프로세스 격리 관행(api_export_bundle.rs 헤더 참조)대로 독립
//! 바이너리로 남긴다: auth_token.rs · api_export_bundle.rs ·
//! detector_config_file.rs. env 판독자(ingest·detector config 등)와 한
//! 프로세스에 합치면 스레드 병렬에서 경쟁하기 때문이다.
//!
//! 새 통합 테스트는 이 디렉터리에 파일을 추가하고 아래에 `mod` 한 줄을
//! 더한다. 단일 모듈만 돌릴 때: `cargo test --test it <mod이름>::`
mod agent_id_backfill;
mod api;
mod api_audit;
mod api_detectors;
mod api_diff_hunks;
mod api_event_raw_redaction;
mod api_events_agent_id;
mod api_events_tag;
mod api_fingerprint;
mod api_health_insight;
mod api_instructions;
mod api_metrics;
mod api_metrics_sessions;
mod api_redaction_summary;
mod api_resource_gone;
mod api_session_list_facts;
mod api_session_turns;
mod api_signals;
mod api_turn_tokens;
mod api_usage;
mod api_usage_baseline;
mod api_verification_runs;
mod api_verification_summary;
mod auth_default_off;
mod auth_middleware;
mod cargo_dep_audit;
mod cli_help;
mod cli_ingest;
mod cli_serve;
mod cli_token_flags;
mod db_init;
mod db_vacuum;
mod detector_config;
mod detector_manifest;
mod doctor;
mod episode_removed;
mod event_kind_no_diffhunk;
mod event_tags;
mod events_subprocess;
mod extractor_context_bloat;
mod extractor_re_read;
mod extractor_risky_action;
mod extractor_tool_failure;
mod fingerprint_compute;
mod health_sources;
mod ingest_applies_redaction;
mod ingest_batch;
mod ingest_findings_no_graph;
mod ingest_recompute_policy;
mod ingest_store;
mod ingest_streaming_verification;
mod ingest_teammate_fields;
mod insight_l1_promotion;
mod insight_pipeline;
mod insight_provenance;
mod instruction_observe;
mod live_event;
mod live_sink;
mod mapping;
mod mcp_initialize;
mod mcp_origin_validation;
mod mcp_resources_list;
mod mcp_resources_read;
mod mcp_resources_read_redaction;
mod mcp_session_ttl;
mod mcp_spec_compat;
mod mcp_sse;
mod mcp_tools_call;
mod mcp_tools_list;
mod metrics_cache;
mod metrics_compute;
mod migration_audit_schema;
mod migration_redaction_manifest_column;
mod migration_retention_schema;
mod migration_schema;
mod migration_verification_run_schema;
mod otel_ingest;
mod otel_log_correlation_columns;
mod otel_logs_ingest;
mod otel_metrics_ingest;
mod outcome_resolve;
mod parser;
mod payload_model;
mod payload_tool_name;
mod plugin_registry;
mod re_read_idempotent;
mod redaction_manifest_shape;
mod redaction_masking;
mod redaction_rule_pack;
mod redaction_shim_lock;
mod redaction_synthetic_fixture;
mod repo_observed;
mod repo_raw;
mod repo_signal;
mod repo_verification_run;
mod repo_window;
mod retention_sweep;
mod retention_sweep_cancel;
mod serve_file_log;
mod serve_shutdown;
mod session_events_api;
mod session_events_filter;
mod session_events_kind_filter;
mod session_facts;
mod sessions_project_filter;
mod sse_integration;
mod sse_subprocess;
mod static_serve;
mod subagent_meta_ingest;
mod surface_language;
mod task_summary;
mod tool_failure_outcome;
mod transcript_disposition;
mod transcript_ingest_diff_hunk;
mod transcript_structured_patch;
mod transcript_tail;
mod transcript_verification_bash;
mod turn_backfill;
mod usage_facet_ingest;
mod verification_bash_allowlist;
mod verification_otel_synth;
mod verification_piped_masking;
mod verification_run_outcome;
mod verification_segment_split;
mod verification_tsc;
