//! `episode` side-table repo (slice-12).
//!
//! Mirrors the shape of `repo_verification_run`: insert, list_session, get.
//! The graph builder reads from this repo after calling the episode classifier,
//! which writes episode rows for every session rebuild.

use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// A row in the `episode` table.
#[derive(Debug, Clone)]
pub struct EpisodeRow {
    pub episode_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub phase: String,
    pub start_event_id: String,
    pub end_event_id: String,
    pub started_at: String,
    pub ended_at: String,
    /// JSON array of node IDs that justify the phase label.
    pub evidence_node_ids: String,
    /// JSON array of versioned rule-IDs, e.g. `["phase_intake_fresh_user_message@v1"]`.
    pub classification_basis: String,
    pub confidence: f64,
    pub summary: Option<String>,
    pub classifier_version: String,
    pub created_at: String,
}

/// Type alias kept parallel to other repos.
pub type NewEpisode = EpisodeRow;

/// Insert a row. Uses `INSERT OR REPLACE` so re-running the classifier over
/// the same session replaces previous rows (last-writer-wins, idempotent by PK).
pub async fn insert(pool: &SqlitePool, row: &NewEpisode) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO episode(
            episode_id, schema_version, session_id, phase,
            start_event_id, end_event_id, started_at, ended_at,
            evidence_node_ids, classification_basis, confidence,
            summary, classifier_version)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.episode_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.phase)
    .bind(&row.start_event_id)
    .bind(&row.end_event_id)
    .bind(&row.started_at)
    .bind(&row.ended_at)
    .bind(&row.evidence_node_ids)
    .bind(&row.classification_basis)
    .bind(row.confidence)
    .bind(&row.summary)
    .bind(&row.classifier_version)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete every episode row for a session.
///
/// Slice 3 B2 — episode ids are content-hashed over
/// `(session_id, phase, start_event_id, end_event_id)`. When a live session
/// grows and rebuilds, the trailing episode's `end_event_id` shifts → a new id,
/// so `insert`'s `INSERT OR REPLACE` ADDS a row instead of replacing the old
/// trailing episode, leaving stale rows behind. `rebuild_session` calls this
/// before re-inserting the fresh set so the rebuild fully replaces the
/// session's episodes (mirrors `repo_graph::delete_session_in_tx` for nodes).
pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM episode WHERE session_id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List all episodes for a session, ordered by `started_at`, then `episode_id`.
pub async fn list_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<EpisodeRow>> {
    let rows = sqlx::query(
        "SELECT episode_id, schema_version, session_id, phase,
                start_event_id, end_event_id, started_at, ended_at,
                evidence_node_ids, classification_basis, confidence,
                summary, classifier_version, created_at
         FROM episode
         WHERE session_id = ?
         ORDER BY started_at, episode_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_row).collect())
}

/// Fetch a single episode by ID.
pub async fn get(pool: &SqlitePool, episode_id: &str) -> Result<Option<EpisodeRow>> {
    let row = sqlx::query(
        "SELECT episode_id, schema_version, session_id, phase,
                start_event_id, end_event_id, started_at, ended_at,
                evidence_node_ids, classification_basis, confidence,
                summary, classifier_version, created_at
         FROM episode
         WHERE episode_id = ?",
    )
    .bind(episode_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(map_row))
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> EpisodeRow {
    EpisodeRow {
        episode_id: r.get("episode_id"),
        schema_version: r.get("schema_version"),
        session_id: r.get("session_id"),
        phase: r.get("phase"),
        start_event_id: r.get("start_event_id"),
        end_event_id: r.get("end_event_id"),
        started_at: r.get("started_at"),
        ended_at: r.get("ended_at"),
        evidence_node_ids: r.get("evidence_node_ids"),
        classification_basis: r.get("classification_basis"),
        confidence: r.get("confidence"),
        summary: r.get("summary"),
        classifier_version: r.get("classifier_version"),
        created_at: r.get("created_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use sqlx::sqlite::SqlitePoolOptions;

    fn sample_row() -> NewEpisode {
        NewEpisode {
            episode_id: "ep_test_001".into(),
            schema_version: "episode.v1".into(),
            session_id: "sess_ep_test".into(),
            phase: "intake".into(),
            start_event_id: "ev_000".into(),
            end_event_id: "ev_000".into(),
            started_at: "2026-05-28T10:00:00Z".into(),
            ended_at: "2026-05-28T10:00:01Z".into(),
            evidence_node_ids: "[]".into(),
            classification_basis: r#"["phase_intake_fresh_user_message@v1"]"#.into(),
            confidence: 1.0,
            summary: None,
            classifier_version: "episode_classifier@v1".into(),
            created_at: "2026-05-28T10:00:02Z".into(),
        }
    }

    #[tokio::test]
    async fn insert_then_list_session() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_row();
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "sess_ep_test").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].episode_id, "ep_test_001");
        assert_eq!(out[0].phase, "intake");
        assert!((out[0].confidence - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn get_returns_correct_row() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_row();
        insert(&pool, &row).await.unwrap();
        let fetched = get(&pool, "ep_test_001").await.unwrap().unwrap();
        assert_eq!(fetched.session_id, "sess_ep_test");
        assert_eq!(fetched.classifier_version, "episode_classifier@v1");
    }

    #[tokio::test]
    async fn get_returns_none_for_missing() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let result = get(&pool, "ep_nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn insert_or_replace_dedup() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        let row = sample_row();
        insert(&pool, &row).await.unwrap();
        insert(&pool, &row).await.unwrap();
        let out = list_session(&pool, "sess_ep_test").await.unwrap();
        assert_eq!(out.len(), 1, "INSERT OR REPLACE must deduplicate by PK");
    }
}
