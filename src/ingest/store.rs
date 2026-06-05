use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::Path;

use crate::db::{repo_diff_hunk, repo_observed, repo_raw, repo_runs, repo_usage_facet, repo_verification_run};
use crate::live::{LiveEvent, LiveSink};
use crate::security::redaction::engine::scan;

/// Row type for the turn_id backfill query: (event_uuid, parent_uuid, turn_id, event_id)
type TurnBackfillRow = (String, Option<String>, Option<String>, Option<String>);
use crate::error::{Result, WimccError};
use crate::ids::MonotonicUlidGen;
use crate::ingest::{diff_hunk, mapping, transcript, usage_facet, verification_run};
use crate::model::observed::EventKind;
use crate::model::meta::SCHEMA_VERSION;

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
    let mut gen = MonotonicUlidGen::new();
    let run_id = repo_runs::start(pool).await?;
    let mut stats = IngestStats::default();
    let mut stream =
        Box::pin(
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
                let redaction_manifest =
                    serde_json::to_string(&scan_result.manifest).ok();
                let inserted = repo_raw::insert_dedup(
                    pool,
                    &repo_raw::NewRaw {
                        raw_event_id: raw_id.clone(),
                        ingest_run_id: run_id.clone(),
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
                    // slice-7: still mark the session as touched so a later
                    // graph rebuild runs. Without this, replaying `ingest --all`
                    // after the graph-rebuild fix landed has no effect on
                    // already-ingested transcripts — every line dedups and
                    // sessions_touched stays empty.
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
                let evs = mapping::map_record(&meta, &rec, &raw_id, &mut gen)?;
                for mut ev in evs {
                    stats.sessions_touched.insert(ev.session_id.clone());
                    // Slice-18: redact observed_event.payload too.
                    // The payload is a JSON Value derived from the parsed record;
                    // its string representation may contain the original secrets.
                    // Apply the same gate so the normalized event is clean.
                    if scan_result.applied {
                        let payload_str = ev.payload.to_string();
                        let redacted_str = scan(&payload_str).masked_text;
                        ev.payload = serde_json::from_str(&redacted_str)
                            .unwrap_or(ev.payload);
                    }
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
                        ingest_run_id: run_id.clone(),
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

    for session_id in &stats.sessions_touched {
        backfill_turn_ids(pool, session_id).await?;

        // Slice-11 — extract verification runs for this session and persist
        // them before rebuild_session reads them. This mirrors the diff_hunk
        // pattern: side-table is written then the graph builder reads it.
        if !session_id.is_empty() {
            let evs = repo_observed::list_session(pool, session_id, 100_000).await?;
            let vr_records = verification_run::extract_verification_runs(&evs);
            for rec in vr_records {
                repo_verification_run::insert(
                    pool,
                    &repo_verification_run::VerificationRunRow {
                        verification_run_id: rec.verification_run_id,
                        schema_version: rec.schema_version.to_string(),
                        session_id: rec.session_id,
                        source: rec.source,
                        command: rec.command,
                        command_kind: rec.command_kind,
                        trigger_event_id: rec.trigger_event_id,
                        trigger_tool_use_id: rec.trigger_tool_use_id,
                        status: rec.status,
                        detection_basis: rec.detection_basis,
                        status_basis: rec.status_basis,
                        started_at: rec.started_at,
                        ended_at: rec.ended_at,
                        exit_code: rec.exit_code,
                        failure_summary: rec.failure_summary,
                        raw_event_id: rec.raw_event_id,
                        parser_version: rec.parser_version.to_string(),
                    },
                )
                .await?;
            }
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

        // slice-7 fix: every touched session must have its graph rebuilt so
        // the WebUI timeline renders markers. Without this, OTel ingest
        // paths populate graph_node but the transcript path does not —
        // SessionDetail shows zero markers even when /v1/sessions/<id>
        // reports hundreds of events.
        crate::graph::build::rebuild_session(pool, session_id).await?;
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
