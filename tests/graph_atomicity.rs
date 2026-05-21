//! Slice-9 — `graph::build::rebuild_session` must be atomic. Before slice-9
//! it ran DELETE-then-INSERT in two separate transactions; a SELECT issued
//! between the two saw 0 rows even though the session had thousands of events
//! (DEV-S8-12). This chaos test exercises the race directly using a
//! file-backed WAL pool so concurrent connections share state for real
//! (in-memory `sqlite::memory:` pools don't share data across connections).
//!
//! Pass criterion: every concurrent SELECT against `graph_node` returns
//! `expected_node_count`. Never zero, never partial.

use chrono::{DateTime, TimeZone, Utc};
use sqlx::Row;
use std::sync::Arc;
use tempfile::tempdir;
use witmcc::db::{connect, migrate, repo_observed, repo_raw, repo_runs};
use witmcc::graph::build::rebuild_session;
use witmcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-atomic";
const SEED_N: usize = 60;
const REBUILD_LOOPS: usize = 12;
const READER_TASKS: usize = 8;
const READS_PER_TASK: usize = 60;

#[tokio::test]
async fn rebuild_session_is_atomic_under_concurrent_reads() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("atomic.db");
    let url = format!("sqlite://{}", db_path.display());
    let pool = connect(&url).await.unwrap();
    migrate(&pool).await.unwrap();

    // 1. Seed observed_event rows so compute() produces a stable, non-zero
    //    graph. UserMessage maps 1:1 to a user_message node (keyed by
    //    event_uuid), so SEED_N rows ⇒ SEED_N nodes.
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    for i in 0..SEED_N {
        let event_id = format!("01J{:023}", i);
        let raw_id = format!("raw_{i:06}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_{i:06}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id,
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: SESS.into(),
            event_uuid: Some(format!("uuid-{i:06}")),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }

    // 2. Prime the graph so readers never legitimately see 0 rows.
    let (initial_nodes, _) = rebuild_session(&pool, SESS).await.unwrap();
    assert_eq!(
        initial_nodes, SEED_N,
        "seed should produce one user_message node per event"
    );

    // 3. Chaos run — one writer that rebuilds REBUILD_LOOPS times in tight
    //    succession, plus READER_TASKS tasks each doing READS_PER_TASK
    //    SELECTs interleaved with brief yields.
    let pool = Arc::new(pool);

    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move {
        for _ in 0..REBUILD_LOOPS {
            rebuild_session(&writer_pool, SESS).await.unwrap();
            // Don't pause — we want the readers landing during every gap.
            tokio::task::yield_now().await;
        }
    });

    let mut readers = Vec::with_capacity(READER_TASKS);
    for _ in 0..READER_TASKS {
        let p = pool.clone();
        readers.push(tokio::spawn(async move {
            let mut observations: Vec<i64> = Vec::with_capacity(READS_PER_TASK);
            for _ in 0..READS_PER_TASK {
                let n: i64 = sqlx::query(
                    "SELECT COUNT(*) AS c FROM graph_node WHERE session_id = ?",
                )
                .bind(SESS)
                .fetch_one(&*p)
                .await
                .unwrap()
                .get("c");
                observations.push(n);
                tokio::task::yield_now().await;
            }
            observations
        }));
    }

    writer.await.unwrap();
    let all_observations: Vec<i64> = futures::future::join_all(readers)
        .await
        .into_iter()
        .flat_map(|r| r.unwrap())
        .collect();

    // 4. The atomicity claim: every observation is either the steady-state
    //    row count or — once the test allows compute mutation — some other
    //    fully-committed count. Slice-9 keeps SEED_N constant, so every read
    //    must equal SEED_N. Zero is the regression we're guarding against.
    let zero_reads = all_observations.iter().filter(|&&n| n == 0).count();
    assert_eq!(
        zero_reads, 0,
        "rebuild_session race observed {} zero-row reads out of {} (DEV-S8-12 regression)",
        zero_reads,
        all_observations.len()
    );
    let consistent = all_observations
        .iter()
        .all(|&n| n == SEED_N as i64);
    assert!(
        consistent,
        "observations should all be SEED_N={SEED_N}; got distinct values: {:?}",
        all_observations
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    );
}
