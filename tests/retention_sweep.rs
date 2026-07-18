//! Retention sweep — full-policy contract (docs/05 §05, decision 2026-06-11).
//!
//! The sweep enforces all four data classes of the active profile:
//! - raw payload: **scrub** (`payload` emptied, skeleton row kept for the
//!   `observed_event.raw_event_id` FK + dedup triple) + tombstone → 410.
//! - normalized + insight: **session granularity** — a session whose newest
//!   `observed_at` is older than the normalized cutoff loses its
//!   observed_event / diff_hunk / verification_run / usage_facet / signal rows.
//!   The session id, signal ids and verification_run ids get tombstones.
//! - audit: rows older than the audit cutoff are deleted.
//!
//! Everything runs in one transaction with cutoffs derived from a single
//! `Utc::now()` so a cancelled sweep rolls back atomically.

use wimcc::security::retention::{Profile, RetentionPolicy};

async fn test_pool() -> sqlx::SqlitePool {
    let pool = wimcc::db::connect(":memory:").await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();
    pool
}

async fn seed_ingest_run(pool: &sqlx::SqlitePool) -> String {
    let run_id = format!("run_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT OR IGNORE INTO ingest_run (run_id, started_at, status) VALUES (?, datetime('now'), 'done')"
    )
    .bind(&run_id)
    .execute(pool)
    .await
    .unwrap();
    run_id
}

/// Seed a raw_event with `captured_at` set to `days_old` days in the past.
/// Returns the raw_event_id.
async fn seed_old_raw_event(pool: &sqlx::SqlitePool, days_old: i64) -> String {
    let run_id = seed_ingest_run(pool).await;
    let id = format!("raw_{days_old}d_{}", ulid::Ulid::new());
    let sha = format!("sha_{id}");
    sqlx::query(
        "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
         VALUES (?, ?, 'claude_transcript', 'test.jsonl', 0, 0, ?, '{\"secret\":\"content\"}', datetime('now', ?))"
    )
    .bind(&id)
    .bind(&run_id)
    .bind(&sha)
    .bind(format!("-{days_old} days"))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Seed an observed_event referencing `raw_event_id` (FK), `days_old` days in
/// the past. Returns the event_id.
async fn seed_observed_event(
    pool: &sqlx::SqlitePool,
    raw_event_id: &str,
    session_id: &str,
    days_old: i64,
) -> String {
    let id = format!("ev_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT INTO observed_event (event_id, raw_event_id, schema_version, session_id, observed_at, actor, kind, payload, parser_version)
         VALUES (?, ?, 'observed_event.v1', ?, datetime('now', ?), 'assistant', 'assistant_message', '{}', 'test')"
    )
    .bind(&id)
    .bind(raw_event_id)
    .bind(session_id)
    .bind(format!("-{days_old} days"))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Seed a full session: raw + observed + diff_hunk + verification_run +
/// usage_facet + signal, all `days_old` days in the past.
/// Returns (signal_id, verification_run_id).
async fn seed_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    days_old: i64,
) -> (String, String) {
    let raw_id = seed_old_raw_event(pool, days_old).await;
    let event_id = seed_observed_event(pool, &raw_id, session_id, days_old).await;

    sqlx::query(
        "INSERT INTO diff_hunk (diff_hunk_id, schema_version, session_id, file_path, change_type, introduced_by_event_id, patch_preview, lines_added, lines_removed)
         VALUES (?, 'diff_hunk.v1', ?, 'src/x.rs', 'modify', ?, '@@', 1, 0)"
    )
    .bind(format!("dh_{}", ulid::Ulid::new()))
    .bind(session_id)
    .bind(&event_id)
    .execute(pool)
    .await
    .unwrap();

    let vr_id = format!("vr_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT INTO verification_run (verification_run_id, session_id, source, command, command_kind, trigger_event_id, status, started_at, raw_event_id, parser_version)
         VALUES (?, ?, 'bash', 'cargo test', 'test_suite_rust', ?, 'passed', datetime('now', ?), ?, 'test')"
    )
    .bind(&vr_id)
    .bind(session_id)
    .bind(&event_id)
    .bind(format!("-{days_old} days"))
    .bind(&raw_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO usage_facet (raw_event_id, session_id, observed_at, parser_version)
         VALUES (?, ?, datetime('now', ?), 'test')",
    )
    .bind(&raw_id)
    .bind(session_id)
    .bind(format!("-{days_old} days"))
    .execute(pool)
    .await
    .unwrap();

    let sig_id = format!("sig_{}", ulid::Ulid::new());
    sqlx::query(
        "INSERT INTO signal (signal_id, session_id, detector, summary, evidence_refs, facts, provenance)
         VALUES (?, ?, 'd_test', 'test signal', '[]', '{}', '{}')",
    )
    .bind(&sig_id)
    .bind(session_id)
    .execute(pool)
    .await
    .unwrap();

    (sig_id, vr_id)
}

async fn count(pool: &sqlx::SqlitePool, sql: &str, bind: &str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(bind)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// raw payload class — scrub, not row delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sweep_default_profile_scrubs_raw_payload_older_than_60d() {
    let pool = test_pool().await;
    let old_id = seed_old_raw_event(&pool, 61).await;
    let new_id = seed_old_raw_event(&pool, 5).await; // must stay intact

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    assert_eq!(
        report.deletions.get("raw_event").copied().unwrap_or(0),
        1,
        "should scrub exactly 1 raw payload older than 60d"
    );

    // Both rows survive: the skeleton (provenance triple) is kept so the
    // observed_event FK and the ingest dedup key stay valid.
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM raw_event")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 2, "scrub keeps skeleton rows");

    let old_len: i64 = count(
        &pool,
        "SELECT length(payload) FROM raw_event WHERE raw_event_id = ?",
        &old_id,
    )
    .await;
    assert_eq!(old_len, 0, "expired payload must be emptied");

    let new_len: i64 = count(
        &pool,
        "SELECT length(payload) FROM raw_event WHERE raw_event_id = ?",
        &new_id,
    )
    .await;
    assert!(new_len > 0, "recent payload must be untouched");
}

#[tokio::test]
async fn scrubbed_raw_payload_has_tombstone() {
    let pool = test_pool().await;
    let id = seed_old_raw_event(&pool, 61).await;

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    let tomb: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM retention_tombstone WHERE resource_id = ? AND resource_kind = 'raw_event'",
        &id,
    )
    .await;
    assert_eq!(tomb, 1, "scrubbed payload should have a tombstone row");
}

/// Regression: production raw rows always have observed_event children
/// (FK `observed_event.raw_event_id`, enforced — `db::connect` sets
/// `foreign_keys(true)`). The old row-DELETE sweep failed with a constraint
/// violation on every real database; the scrub must succeed.
#[tokio::test]
async fn raw_scrub_succeeds_with_observed_event_fk_child() {
    let pool = test_pool().await;
    let raw_id = seed_old_raw_event(&pool, 61).await;
    // Child is recent → its session is NOT expired, so the raw row cannot be
    // deleted without breaking the FK. Scrub must still empty the payload.
    seed_observed_event(&pool, &raw_id, "sess_fk_child", 1).await;

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .expect("sweep must not trip the observed_event FK");

    assert_eq!(report.deletions.get("raw_event").copied().unwrap_or(0), 1);
    let len: i64 = count(
        &pool,
        "SELECT length(payload) FROM raw_event WHERE raw_event_id = ?",
        &raw_id,
    )
    .await;
    assert_eq!(len, 0);
}

#[tokio::test]
async fn raw_scrub_is_idempotent_across_sweeps() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 61).await;

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let first = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();
    assert_eq!(first.deletions.get("raw_event").copied().unwrap_or(0), 1);

    let second = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();
    assert_eq!(
        second.deletions.get("raw_event").copied().unwrap_or(0),
        0,
        "already-scrubbed payloads must not be re-counted"
    );

    let tombs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM retention_tombstone WHERE resource_kind = 'raw_event'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tombs, 1);
}

#[tokio::test]
async fn sweep_strict_profile_scrubs_raw_older_than_7d() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 8).await; // scrubbed (strict = 7d)
    seed_old_raw_event(&pool, 3).await; // kept

    let p = RetentionPolicy {
        profile: Profile::Strict,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    assert_eq!(
        report.deletions.get("raw_event").copied().unwrap_or(0),
        1,
        "strict profile should scrub raw payloads older than 7d"
    );
}

// ---------------------------------------------------------------------------
// none profile
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sweep_none_profile_deletes_nothing() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 365).await;
    seed_session(&pool, "sess_ancient", 400).await;

    let p = RetentionPolicy {
        profile: Profile::None,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    let total_deleted: u64 = report.deletions.values().sum();
    assert_eq!(total_deleted, 0, "none profile should delete nothing");

    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observed_event")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 1, "none profile must not touch normalized rows");
}

// ---------------------------------------------------------------------------
// normalized + insight classes — session granularity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn expired_session_loses_normalized_and_insight_rows() {
    let pool = test_pool().await;
    let (sig_id, vr_id) = seed_session(&pool, "sess_old", 200).await; // > 180d
    seed_session(&pool, "sess_new", 5).await; // kept

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    for (table, key) in [
        ("observed_event", "observed_event"),
        ("diff_hunk", "diff_hunk"),
        ("verification_run", "verification_run"),
        ("usage_facet", "usage_facet"),
        ("signal", "signal"),
    ] {
        let old: i64 = count(
            &pool,
            &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?"),
            "sess_old",
        )
        .await;
        assert_eq!(old, 0, "{table}: expired session rows must be deleted");
        let new: i64 = count(
            &pool,
            &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?"),
            "sess_new",
        )
        .await;
        assert_eq!(new, 1, "{table}: live session rows must be kept");
        assert_eq!(
            report.deletions.get(key).copied().unwrap_or(0),
            1,
            "{key}: report must count the deleted row"
        );
    }
    assert_eq!(report.deletions.get("session").copied().unwrap_or(0), 1);

    // Tombstones: session, signal, verification_run get one each.
    for (id, kind) in [
        ("sess_old", "session"),
        (sig_id.as_str(), "signal"),
        (vr_id.as_str(), "verification_run"),
    ] {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM retention_tombstone WHERE resource_id = ? AND resource_kind = ?",
        )
        .bind(id)
        .bind(kind)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "{kind} {id} should be tombstoned");
    }
}

#[tokio::test]
async fn session_with_recent_activity_is_kept_whole() {
    let pool = test_pool().await;
    // Old events but a recent one too: MAX(observed_at) is recent → the whole
    // session stays (session granularity, not row granularity).
    seed_session(&pool, "sess_mixed", 200).await;
    let raw = seed_old_raw_event(&pool, 1).await;
    seed_observed_event(&pool, &raw, "sess_mixed", 1).await;

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    assert_eq!(report.deletions.get("session").copied().unwrap_or(0), 0);
    let events: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM observed_event WHERE session_id = ?",
        "sess_mixed",
    )
    .await;
    assert_eq!(events, 2, "old rows of a live session must survive");
}

/// The sweep treats normalized + insight as one session-granularity class;
/// this only holds while their retention windows agree. If this test ever
/// fails, the sweep needs a separate insight pass.
#[test]
fn insight_window_matches_normalized_window() {
    for p in [Profile::Default, Profile::Strict, Profile::None] {
        assert_eq!(p.insight_days(), p.normalized_event_days());
    }
}

// ---------------------------------------------------------------------------
// audit class
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_rows_older_than_window_are_deleted() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO audit (audit_id, event, actor, created_at) VALUES ('aud_old', 'rotate.token', 'owner', datetime('now', '-100 days'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO audit (audit_id, event, actor, created_at) VALUES ('aud_new', 'rotate.token', 'owner', datetime('now', '-5 days'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let p = RetentionPolicy {
        profile: Profile::Default, // audit 90d
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    assert_eq!(report.deletions.get("audit").copied().unwrap_or(0), 1);
    let old: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE audit_id = 'aud_old'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old, 0);
    let new: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE audit_id = 'aud_new'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(new, 1);
}

/// A session can exist only in side tables (no observed_event rows) — e.g.
/// partially ingested historical data. Expiry must be judged by
/// session-activity timestamps (`verification_run.started_at`,
/// `usage_facet.observed_at`), and such sessions must still be swept.
/// `signal.created_at` is ingest time, NOT session activity — it must not
/// keep the session alive (re-ingest is routine and would otherwise reset
/// retention forever). The signal row here keeps its DEFAULT (= now) to lock
/// exactly that.
#[tokio::test]
async fn orphan_session_without_observed_events_is_swept() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO verification_run (verification_run_id, session_id, source, command, command_kind, trigger_event_id, status, started_at, raw_event_id, parser_version)
         VALUES ('vr_orphan', 'sess_orphan', 'bash', 'cargo test', 'test_suite_rust', 'ev_gone', 'passed', datetime('now', '-200 days'), 'raw_gone', 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO usage_facet (raw_event_id, session_id, observed_at, parser_version)
         VALUES ('raw_orphan_uf', 'sess_orphan', datetime('now', '-200 days'), 'test')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO signal (signal_id, session_id, detector, summary, evidence_refs, facts, provenance)
         VALUES ('sig_orphan', 'sess_orphan', 'd_test', 'orphan', '[]', '{}', '{}')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    assert_eq!(
        report.deletions.get("session").copied().unwrap_or(0),
        1,
        "orphan session must be selected for expiry"
    );
    for table in ["verification_run", "usage_facet", "signal"] {
        let n: i64 = count(
            &pool,
            &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?"),
            "sess_orphan",
        )
        .await;
        assert_eq!(n, 0, "{table}: orphan session rows must be deleted");
    }
    for (id, kind) in [
        ("sess_orphan", "session"),
        ("sig_orphan", "signal"),
        ("vr_orphan", "verification_run"),
    ] {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM retention_tombstone WHERE resource_id = ? AND resource_kind = ?",
        )
        .bind(id)
        .bind(kind)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "{kind} {id} should be tombstoned");
    }
}

// ---------------------------------------------------------------------------
// 2026-07-18 growth sweep — tables added after slice-19 must not be grow-only
// (dogfood DB audit: session_summary / instruction_observation / ingest_run
// accumulated forever because the sweep predates them).
// ---------------------------------------------------------------------------

/// `session_summary` (0025) and `instruction_observation` (0028) are
/// session-scoped derived/observation tables — they expire with the session.
/// `instruction_snapshot` is content-addressed and shared; a snapshot loses
/// its rows only when NO surviving observation references it.
#[tokio::test]
async fn expired_session_loses_summary_and_instruction_rows() {
    let pool = test_pool().await;
    seed_session(&pool, "sess_old", 200).await;
    seed_session(&pool, "sess_new", 1).await;

    for (sid, sha, days) in [
        ("sess_old", "sha_only_old", 200i64),
        ("sess_new", "sha_shared", 1),
        ("sess_old", "sha_shared", 200),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO instruction_snapshot (content_sha256, content, first_observed_at)
             VALUES (?, 'content', datetime('now', ?))",
        )
        .bind(sha)
        .bind(format!("-{days} days"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO instruction_observation (observation_id, session_id, source, path, content_sha256, observed_at)
             VALUES (?, ?, 'project', 'CLAUDE.md', ?, datetime('now', ?))",
        )
        .bind(format!("obs_{}", ulid::Ulid::new()))
        .bind(sid)
        .bind(sha)
        .bind(format!("-{days} days"))
        .execute(&pool)
        .await
        .unwrap();
    }
    for (sid, days) in [("sess_old", 200i64), ("sess_new", 1)] {
        sqlx::query(
            "INSERT INTO session_summary (session_id, project, updated_at)
             VALUES (?, 'proj', datetime('now', ?))",
        )
        .bind(sid)
        .bind(format!("-{days} days"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    for table in ["session_summary", "instruction_observation"] {
        let old: i64 = count(
            &pool,
            &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?"),
            "sess_old",
        )
        .await;
        assert_eq!(old, 0, "{table}: expired session rows must be deleted");
        let new: i64 = count(
            &pool,
            &format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?"),
            "sess_new",
        )
        .await;
        assert_eq!(new, 1, "{table}: live session rows must be kept");
    }
    let orphan: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM instruction_snapshot WHERE content_sha256 = ?",
        "sha_only_old",
    )
    .await;
    assert_eq!(orphan, 0, "snapshot no observation references must be pruned");
    let shared: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM instruction_snapshot WHERE content_sha256 = ?",
        "sha_shared",
    )
    .await;
    assert_eq!(shared, 1, "snapshot still referenced by a live session stays");
}

/// A session whose only trace is instruction_observation rows (e.g. all its
/// events already expired in an earlier sweep) must still expire — the
/// candidate scan reads every table carrying a session-activity timestamp.
#[tokio::test]
async fn session_with_only_instruction_rows_expires() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO instruction_snapshot (content_sha256, content, first_observed_at)
         VALUES ('sha_lonely', 'c', datetime('now', '-200 days'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO instruction_observation (observation_id, session_id, source, path, content_sha256, observed_at)
         VALUES ('obs_lonely', 'sess_lonely', 'project', 'CLAUDE.md', 'sha_lonely', datetime('now', '-200 days'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    let n: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM instruction_observation WHERE session_id = ?",
        "sess_lonely",
    )
    .await;
    assert_eq!(n, 0, "instruction-only session must expire by observed_at");
}

/// `ingest_run` rows older than the audit window are pruned when no raw_event
/// references them (empty live flushes, historical accumulation). Referenced
/// rows stay — raw skeleton rows carry an FK to them.
#[tokio::test]
async fn old_unreferenced_ingest_run_rows_are_pruned() {
    let pool = test_pool().await;
    // referenced old run: seed_old_raw_event creates run + raw row (captured 200d ago
    // scrubs the payload but keeps the skeleton + its run).
    let raw_id = seed_old_raw_event(&pool, 200).await;
    // unreferenced old run
    sqlx::query(
        "INSERT INTO ingest_run (run_id, started_at, status) VALUES ('run_empty_old', datetime('now', '-200 days'), 'ok')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // unreferenced recent run
    sqlx::query(
        "INSERT INTO ingest_run (run_id, started_at, status) VALUES ('run_empty_new', datetime('now', '-1 days'), 'ok')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    let report = wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    let empty_old: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM ingest_run WHERE run_id = ?",
        "run_empty_old",
    )
    .await;
    assert_eq!(empty_old, 0, "old unreferenced run must be pruned");
    let empty_new: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM ingest_run WHERE run_id = ?",
        "run_empty_new",
    )
    .await;
    assert_eq!(empty_new, 1, "recent unreferenced run stays");
    let referenced: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ingest_run WHERE run_id = (SELECT ingest_run_id FROM raw_event WHERE raw_event_id = ?)",
    )
    .bind(&raw_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(referenced, 1, "run referenced by a raw skeleton row stays (FK)");
    assert!(
        report.deletions.get("ingest_run").copied().unwrap_or(0) >= 1,
        "report counts pruned ingest_run rows; got {:?}",
        report.deletions
    );
}

/// Disk reclaim: deleting rows never shrank the .sqlite file (no VACUUM
/// anywhere — the 1.2GB dogfood file can only grow). New DBs get
/// auto_vacuum=INCREMENTAL and the sweep releases freelist pages afterwards,
/// so the main DB file actually shrinks on a file-backed DB.
#[tokio::test]
async fn sweep_reclaims_disk_on_file_backed_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reclaim.sqlite");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = wimcc::db::connect(&url).await.unwrap();
    wimcc::db::migrate(&pool).await.unwrap();

    let av: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(av, 2, "new DBs must be created with auto_vacuum=INCREMENTAL");

    // ~2MB of expired payload across 100 raw rows.
    let run_id = seed_ingest_run(&pool).await;
    let blob = "x".repeat(20_000);
    for i in 0..100 {
        sqlx::query(
            "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
             VALUES (?, ?, 'claude_transcript', 'big.jsonl', ?, 0, ?, ?, datetime('now', '-100 days'))",
        )
        .bind(format!("raw_big_{i}"))
        .bind(&run_id)
        .bind(i as i64)
        .bind(format!("sha_big_{i}"))
        .bind(&blob)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Move WAL content into the main file so the before-size is honest.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .unwrap();
    let before = std::fs::metadata(&path).unwrap().len();

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    let after = std::fs::metadata(&path).unwrap().len();
    assert!(
        after < before,
        "sweep must release freed pages back to the filesystem; before={before} after={after}"
    );
}

#[tokio::test]
async fn sweep_writes_audit_row() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 61).await;

    let p = RetentionPolicy {
        profile: Profile::Default,
    };
    wimcc::security::retention::run_sweep(&pool, &p)
        .await
        .unwrap();

    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit WHERE event = 'retention.deleted'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "sweep should write exactly one audit row per run");
}
