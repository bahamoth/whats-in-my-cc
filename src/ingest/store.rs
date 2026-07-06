use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::Path;

use crate::db::{
    repo_diff_hunk, repo_observed, repo_raw, repo_runs, repo_usage_facet, repo_verification_run,
};
use crate::live::{LiveEvent, LiveSink};
use crate::security::redaction::engine::scan;

/// Row type for the turn_id backfill query: (event_uuid, parent_uuid, turn_id, event_id)
type TurnBackfillRow = (String, Option<String>, Option<String>, Option<String>);
use crate::error::{Result, WimccError};
use crate::ids::MonotonicUlidGen;
use crate::ingest::{diff_hunk, mapping, subagent_meta, transcript, usage_facet, verification_run};
use crate::model::meta::SCHEMA_VERSION;
use crate::model::observed::{Actor, EventKind, ObservedEvent};

#[derive(Debug, Default, Serialize)]
pub struct IngestStats {
    pub raw_inserted: u64,
    pub raw_skipped: u64,
    pub observed_inserted: u64,
    pub parse_errors: u64,
    pub sessions_touched: std::collections::BTreeSet<String>,
}

pub async fn ingest_file(
    pool: &SqlitePool,
    path: &Path,
    sink: &dyn LiveSink,
) -> Result<IngestStats> {
    ingest_paths(pool, &[path.to_path_buf()], sink).await
}

/// Ingest one or more transcript files in a single run. All raw lines land
/// first; each touched session's insights are then recomputed EXACTLY ONCE over
/// the session's full event set — not once per file. Dogfooding 2026-06-11:
/// `ingest --all` previously re-ran a session's insight pipeline for every
/// subagent file it touched (e.g. 58×). `recompute_session` is idempotent, so
/// 1× over the union equals the old last-file recompute — results unchanged.
pub async fn ingest_paths(
    pool: &SqlitePool,
    paths: &[std::path::PathBuf],
    sink: &dyn LiveSink,
) -> Result<IngestStats> {
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut stats = IngestStats::default();
    for path in paths {
        ingest_one_file(pool, path, sink, &mut gen, &run_id, &mut stats).await?;
    }
    for session_id in &stats.sessions_touched {
        recompute_session(pool, session_id).await?;
    }
    repo_runs::finish(
        pool,
        &run_id,
        "ok",
        serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null),
    )
    .await?;
    Ok(stats)
}

/// Ingest a single transcript file's raw lines (+ observed/diff_hunk rows). No
/// insight recompute — the caller recomputes each touched session ONCE after a
/// batch of files lands (see `ingest_paths`).
async fn ingest_one_file(
    pool: &SqlitePool,
    path: &Path,
    sink: &dyn LiveSink,
    gen: &mut MonotonicUlidGen,
    run_id: &str,
    stats: &mut IngestStats,
) -> Result<()> {
    // Subagent 사이드카(meta.json)는 JSONL이 아니라 단일 JSON 파일 — 전용 경로.
    if let Some(sc) = subagent_meta::sidecar_path_parts(path) {
        return ingest_sidecar_file(pool, path, &sc, sink, gen, run_id, stats).await;
    }
    tracing::info!(path = %path.display(), "ingesting");
    let mut stream = Box::pin(
        transcript::stream_file(path)
            .await
            .map_err(|e| WimccError::Io {
                path: path.to_path_buf(),
                source: e,
            })?,
    );

    while let Some(item) = stream.next().await {
        match item {
            Ok((meta, rec)) => {
                // Slice-18: apply redaction gate before storing. Payload is
                // text (UTF-8 JSON), so we scan the lossy string representation.
                let payload_text = String::from_utf8_lossy(&meta.raw);
                let scan_result = scan(&payload_text);
                let stored_payload: Vec<u8> = if scan_result.applied {
                    scan_result.masked_text.into_bytes()
                } else {
                    meta.raw.clone()
                };
                let payload_sha = hex::encode(Sha256::digest(&stored_payload));
                let raw_id = gen.generate();
                let redaction_state = scan_result.manifest.redaction_state.as_str().to_owned();
                let redaction_manifest = serde_json::to_string(&scan_result.manifest).ok();
                let inserted = repo_raw::insert_dedup(
                    pool,
                    &repo_raw::NewRaw {
                        raw_event_id: raw_id.clone(),
                        ingest_run_id: run_id.to_string(),
                        source_type: "claude_transcript".into(),
                        source_uri: meta.source_uri.display().to_string(),
                        source_line_no: meta.line_no as i64,
                        source_byte_offset: meta.byte_offset as i64,
                        payload_sha256: payload_sha,
                        payload: stored_payload,
                        parse_error: None,
                        captured_at: Utc::now(),
                        redaction_state,
                        redaction_manifest,
                    },
                )
                .await?;
                if !inserted {
                    stats.raw_skipped += 1;
                    // Still mark the session as touched so the insight pipeline
                    // re-runs. Without this, replaying `ingest --all` on
                    // already-ingested transcripts has no effect — every line
                    // dedups and sessions_touched stays empty.
                    if let Some(sid) = rec.session_id() {
                        stats.sessions_touched.insert(sid.to_string());
                    }
                    continue;
                }
                stats.raw_inserted += 1;
                // Slice-10a: pluck out toolUseResult before move so we can
                // populate diff_hunk after the matching tool_result event lands.
                let tu_result: Option<serde_json::Value> = match &rec {
                    transcript::ParsedRecord::User(u) => u.tool_use_result.clone(),
                    _ => None,
                };
                let evs = mapping::map_record(&meta, &rec, &raw_id, gen)?;
                for mut ev in evs {
                    stats.sessions_touched.insert(ev.session_id.clone());
                    // Slice-18: redact observed_event.payload too.
                    // The payload is a JSON Value derived from the parsed record;
                    // its string representation may contain the original secrets.
                    // Apply the same gate so the normalized event is clean.
                    if scan_result.applied {
                        let payload_str = ev.payload.to_string();
                        let redacted_str = scan(&payload_str).masked_text;
                        ev.payload = serde_json::from_str(&redacted_str).unwrap_or(ev.payload);
                    }
                    // Workflow-tool subagents live under <session>/subagents/workflows/
                    // <runId>/; the runId is in the file path (not the record). Capture
                    // it as the deterministic workflow group key.
                    ev.workflow_run_id = subagent_meta::workflow_run_id_from_path(&meta.source_uri);
                    repo_observed::insert(pool, &ev).await?;
                    stats.observed_inserted += 1;
                    sink.emit(LiveEvent {
                        schema_version: LiveEvent::SCHEMA_VERSION.into(),
                        session_id: ev.session_id.clone(),
                        event_id: ev.event_id.clone(),
                        kind: ev.kind,
                        source_type: "transcript".into(),
                        observed_at: ev.observed_at.to_rfc3339(),
                    });
                    // Slice-10a — file lineage. tool_result ObservedEvents with
                    // a toolUseResult.structuredPatch produce one diff_hunk
                    // row per hunk. Write tool_results carry an empty patch
                    // by design and yield zero rows.
                    if ev.kind == EventKind::ToolResult {
                        if let Some(tu) = tu_result.as_ref() {
                            let hunks = diff_hunk::extract_diff_hunks(
                                &ev.event_id,
                                ev.tool_use_id.as_deref(),
                                &ev.session_id,
                                tu,
                            );
                            for h in hunks {
                                repo_diff_hunk::insert(
                                    pool,
                                    &repo_diff_hunk::NewDiffHunk {
                                        diff_hunk_id: h.diff_hunk_id,
                                        schema_version: SCHEMA_VERSION.into(),
                                        session_id: h.session_id,
                                        file_path: h.file_path,
                                        change_type: h.change_type,
                                        line_range_after_start: h
                                            .line_range_after
                                            .map(|(s, _)| s as i64),
                                        line_range_after_end: h
                                            .line_range_after
                                            .map(|(_, e)| e as i64),
                                        introduced_by_event_id: h.introduced_by_event_id,
                                        introduced_by_tool_use_id: h.introduced_by_tool_use_id,
                                        patch_preview: h.patch_preview,
                                        lines_added: h.lines_added as i64,
                                        lines_removed: h.lines_removed as i64,
                                        user_modified: h.user_modified,
                                    },
                                )
                                .await?;
                            }
                        }
                    }
                }
            }
            Err(WimccError::ParseLine {
                source_uri,
                line_no,
                message,
            }) => {
                stats.parse_errors += 1;
                let raw_id = gen.generate();
                let _ = repo_raw::insert_dedup(
                    pool,
                    &repo_raw::NewRaw {
                        raw_event_id: raw_id,
                        ingest_run_id: run_id.to_string(),
                        source_type: "unparseable".into(),
                        source_uri,
                        source_line_no: line_no as i64,
                        source_byte_offset: 0,
                        payload_sha256: format!("err-{line_no}"),
                        payload: b"".to_vec(),
                        parse_error: Some(message),
                        captured_at: Utc::now(),
                        redaction_state: "not_applicable".into(),
                        redaction_manifest: None,
                    },
                )
                .await?;
            }
            Err(other) => return Err(other),
        }
    }
    Ok(())
}

/// Subagent 사이드카 meta.json 한 파일 → raw_event 1행(source-preserving,
/// redaction 경로 통과) + ObservedEvent 1건. 이벤트는 correlation 키 자리에
/// 그대로 싣는다: `agent_id`(사이드체인 그룹 조인) + `tool_use_id`(메인 체인
/// Task tool_call로 점프). `observed_at`은 파일 mtime — 레코드에 타임스탬프가
/// 없는 정적 사이드카라 capture에 가장 가까운 시각이다.
async fn ingest_sidecar_file(
    pool: &SqlitePool,
    path: &Path,
    sc: &subagent_meta::SidecarRef,
    sink: &dyn LiveSink,
    gen: &mut MonotonicUlidGen,
    run_id: &str,
    stats: &mut IngestStats,
) -> Result<()> {
    let bytes = std::fs::read(path).map_err(|e| WimccError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let payload_text = String::from_utf8_lossy(&bytes);
    let parsed: Option<serde_json::Value> = serde_json::from_str(&payload_text).ok();

    let scan_result = scan(&payload_text);
    let stored_payload: Vec<u8> = if scan_result.applied {
        scan_result.masked_text.into_bytes()
    } else {
        bytes.clone()
    };
    let payload_sha = hex::encode(Sha256::digest(&stored_payload));
    let raw_id = gen.generate();
    let inserted = repo_raw::insert_dedup(
        pool,
        &repo_raw::NewRaw {
            raw_event_id: raw_id.clone(),
            ingest_run_id: run_id.to_string(),
            source_type: "claude_subagent_meta".into(),
            source_uri: path.display().to_string(),
            source_line_no: 0,
            source_byte_offset: 0,
            payload_sha256: payload_sha,
            payload: stored_payload,
            parse_error: if parsed.is_none() {
                Some("invalid JSON in subagent meta sidecar".into())
            } else {
                None
            },
            captured_at: Utc::now(),
            redaction_state: scan_result.manifest.redaction_state.as_str().to_owned(),
            redaction_manifest: serde_json::to_string(&scan_result.manifest).ok(),
        },
    )
    .await?;
    if !inserted {
        stats.raw_skipped += 1;
        stats.sessions_touched.insert(sc.session_id.clone());
        return Ok(());
    }
    stats.raw_inserted += 1;
    let Some(mut record) = parsed else {
        stats.parse_errors += 1;
        return Ok(());
    };
    if scan_result.applied {
        let redacted = scan(&record.to_string()).masked_text;
        record = serde_json::from_str(&redacted).unwrap_or(record);
    }
    let tool_use_id = record
        .get("toolUseId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let observed_at = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(chrono::DateTime::<Utc>::from)
        .unwrap_or_else(|_| Utc::now());
    let ev = ObservedEvent {
        event_id: gen.generate(),
        raw_event_id: raw_id,
        schema_version: SCHEMA_VERSION.into(),
        parser_version: subagent_meta::PARSER_VERSION.into(),
        session_id: sc.session_id.clone(),
        observed_at,
        actor: Actor::System,
        kind: EventKind::AttachmentMeta,
        subkind: Some("subagent_meta".into()),
        tool_use_id,
        agent_id: Some(sc.agent_id.clone()),
        workflow_run_id: sc.workflow_run_id.clone(),
        is_sidechain: true,
        // payload는 사이드카 JSON 전체 — unknown field 보존 원칙.
        payload: record,
        ..Default::default()
    };
    stats.sessions_touched.insert(ev.session_id.clone());
    repo_observed::insert(pool, &ev).await?;
    stats.observed_inserted += 1;
    sink.emit(LiveEvent {
        schema_version: LiveEvent::SCHEMA_VERSION.into(),
        session_id: ev.session_id.clone(),
        event_id: ev.event_id.clone(),
        kind: ev.kind,
        source_type: "transcript".into(),
        observed_at: ev.observed_at.to_rfc3339(),
    });
    Ok(())
}

/// Recompute a single session's derived insights from its full event set.
/// Idempotent (list_session-based reads + insert_or_replace / raw_event_id
/// dedupe / dedup_key+reconcile), so calling it once per touched session after
/// a batch of raw inserts yields the same result as the old per-file recompute.
async fn recompute_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    backfill_turn_ids(pool, session_id).await?;

    // Slice-11 — extract verification runs for this session and persist
    // them before the insight pipeline reads them (the pipeline's view
    // loads verification_run + diff_hunk side-tables).
    // 산출은 세션 단위 원자 교체(replace_session) — 스트리밍 중 tool_call만
    // 있던 슬라이스의 행(trigger=call)은 result 도착 후 trigger=result로
    // vr_id가 달라져 insert(PK REPLACE)로는 안 지워진다(2026-07-06 실사고,
    // tests/ingest_streaming_verification.rs).
    if !session_id.is_empty() {
        let evs = repo_observed::list_session(pool, session_id, 100_000).await?;
        let vr_rows: Vec<repo_verification_run::VerificationRunRow> =
            verification_run::extract_verification_runs(&evs)
                .into_iter()
                .map(|rec| repo_verification_run::VerificationRunRow {
                    verification_run_id: rec.verification_run_id,
                    schema_version: rec.schema_version.to_string(),
                    session_id: rec.session_id,
                    source: rec.source,
                    command: rec.command,
                    command_kind: rec.command_kind,
                    trigger_event_id: rec.trigger_event_id,
                    trigger_tool_use_id: rec.trigger_tool_use_id,
                    status: rec.status,
                    status_provenance: rec.status_provenance,
                    detection_basis: rec.detection_basis,
                    status_basis: rec.status_basis,
                    started_at: rec.started_at,
                    ended_at: rec.ended_at,
                    exit_code: rec.exit_code,
                    failure_summary: rec.failure_summary,
                    raw_event_id: rec.raw_event_id,
                    parser_version: rec.parser_version.to_string(),
                })
                .collect();
        repo_verification_run::replace_session(pool, session_id, &vr_rows).await?;
    }

    // insight-redesign #1 — populate usage_facet from raw transcript lines.
    // Usage lives only in raw_event.payload, so we read the joined raw line
    // and parse it; dedupe is by raw_event_id (one assistant turn = one row).
    if !session_id.is_empty() {
        let lines = repo_usage_facet::assistant_raw_lines(pool, session_id).await?;
        for line in lines {
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line.raw) else {
                continue;
            };
            let Some(u) = usage_facet::parse_usage(&val) else {
                continue;
            };
            repo_usage_facet::insert(
                pool,
                &repo_usage_facet::UsageFacetRow {
                    raw_event_id: line.raw_event_id,
                    schema_version: usage_facet::SCHEMA_VERSION.to_string(),
                    session_id: line.session_id,
                    model: u.model.or(line.model),
                    input_tokens: u.input_tokens,
                    cache_creation_input_tokens: u.cache_creation_input_tokens,
                    cache_read_input_tokens: u.cache_read_input_tokens,
                    output_tokens: u.output_tokens,
                    observed_at: line.observed_at,
                    parser_version: usage_facet::PARSER_VERSION.to_string(),
                },
            )
            .await?;
        }
    }

    // Run the deterministic signal detector pipeline so signals are refreshed
    // immediately after ingest. (Previously a side effect of the removed graph rebuild.)
    crate::insight::pipeline::run_detectors(pool, session_id).await?;

    // perf-2026-06-29 — refresh this session's materialized identity facets
    // (project/model/slug/preview) so GET /v1/sessions reads them from
    // session_summary instead of re-scanning observed_event on every request.
    repo_observed::upsert_session_summary(pool, session_id).await?;
    Ok(())
}

pub async fn backfill_turn_ids(pool: &SqlitePool, session_id: &str) -> Result<u64> {
    // Walk parent_uuid chains in memory; cheap enough for slice-1 single-session sizes.
    let rows: Vec<TurnBackfillRow> = sqlx::query_as(
        "SELECT event_uuid, parent_uuid, turn_id, event_id
         FROM observed_event WHERE session_id = ? AND event_uuid IS NOT NULL",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    use std::collections::HashMap;
    let parent_of: HashMap<String, Option<String>> = rows
        .iter()
        .map(|(uuid, parent, _t, _eid)| (uuid.clone(), parent.clone()))
        .collect();
    let prompt_of: HashMap<String, String> = {
        let r: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT event_uuid, turn_id FROM observed_event
             WHERE session_id = ? AND event_uuid IS NOT NULL AND turn_id IS NOT NULL AND turn_id != ''")
            .bind(session_id).fetch_all(pool).await?;
        r.into_iter()
            .filter_map(|(u, p)| p.map(|p| (u, p)))
            .collect()
    };

    let mut updates: Vec<(String, String)> = Vec::new(); // (event_id, turn_id)
    for (uuid, _parent, turn_id, event_id) in &rows {
        // Skip rows that already have a non-empty turn_id.
        if turn_id.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
            continue;
        }
        let mut cur = parent_of.get(uuid).cloned().flatten();
        let mut found: Option<String> = None;
        let mut hops = 0usize;
        while let Some(p) = cur {
            if let Some(pid) = prompt_of.get(&p) {
                found = Some(pid.clone());
                break;
            }
            cur = parent_of.get(&p).cloned().flatten();
            hops += 1;
            if hops > 256 {
                break;
            } // cycle guard
        }
        if let Some(tid) = found {
            updates.push((event_id.clone().unwrap_or_default(), tid));
        }
    }

    let mut tx = pool.begin().await?;
    let mut applied = 0u64;
    for (event_id, turn_id) in updates {
        sqlx::query("UPDATE observed_event SET turn_id = ? WHERE event_id = ?")
            .bind(&turn_id)
            .bind(&event_id)
            .execute(&mut *tx)
            .await?;
        applied += 1;
    }
    tx.commit().await?;
    Ok(applied)
}
