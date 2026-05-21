use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::Path;

use crate::db::{repo_observed, repo_raw, repo_runs};
use crate::live::{LiveEvent, LiveSink};

/// Row type for the turn_id backfill query: (event_uuid, parent_uuid, turn_id, event_id)
type TurnBackfillRow = (String, Option<String>, Option<String>, Option<String>);
use crate::error::{Result, WitmccError};
use crate::ids::MonotonicUlidGen;
use crate::ingest::{mapping, transcript};

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
                .map_err(|e| WitmccError::Io {
                    path: path.to_path_buf(),
                    source: e,
                })?,
        );

    while let Some(item) = stream.next().await {
        match item {
            Ok((meta, rec)) => {
                let payload_sha = hex::encode(Sha256::digest(&meta.raw));
                let raw_id = gen.generate();
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
                        payload: meta.raw.clone(),
                        parse_error: None,
                        captured_at: Utc::now(),
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
                let evs = mapping::map_record(&meta, &rec, &raw_id, &mut gen)?;
                for ev in evs {
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
                }
            }
            Err(WitmccError::ParseLine {
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
                    },
                )
                .await?;
            }
            Err(other) => return Err(other),
        }
    }

    for session_id in &stats.sessions_touched {
        backfill_turn_ids(pool, session_id).await?;
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
