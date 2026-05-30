//! Repository for the usage_facet side-table.
use sqlx::{Row, SqlitePool};

use crate::error::Result;

/// A usage_facet row ready for insertion.
#[derive(Debug, Clone, Default)]
pub struct UsageFacetRow {
    pub raw_event_id: String,
    pub schema_version: String,
    pub session_id: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub observed_at: String,
    pub parser_version: String,
}

/// One assistant raw transcript line for a session, deduped by raw_event_id.
/// `raw` is the full `raw_event.payload` JSON text; `model` is the cheap copy
/// already present on `observed_event.payload`.
#[derive(Debug, Clone)]
pub struct AssistantRawLine {
    pub raw_event_id: String,
    pub session_id: String,
    pub observed_at: String,
    pub model: Option<String>,
    pub raw: String,
}

/// Aggregate over a session's usage_facet rows.
#[derive(Debug, Clone, Default)]
pub struct UsageAggregate {
    pub turns: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub by_model: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub model: String,
    pub turns: i64,
    pub output_tokens: i64,
}

pub async fn insert(pool: &SqlitePool, row: &UsageFacetRow) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO usage_facet(
            raw_event_id, schema_version, session_id, model,
            input_tokens, cache_creation_input_tokens, cache_read_input_tokens,
            output_tokens, observed_at, parser_version)
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&row.raw_event_id)
    .bind(&row.schema_version)
    .bind(&row.session_id)
    .bind(&row.model)
    .bind(row.input_tokens)
    .bind(row.cache_creation_input_tokens)
    .bind(row.cache_read_input_tokens)
    .bind(row.output_tokens)
    .bind(&row.observed_at)
    .bind(&row.parser_version)
    .execute(pool)
    .await?;
    Ok(())
}

/// Distinct assistant raw lines for a session (one per raw_event_id), so the
/// caller can parse usage from `raw`. Usage lives only in raw_event.payload.
/// Filters by `actor = 'assistant'` (not `kind = 'assistant_message'`) because
/// an assistant transcript line that contains only tool_use content produces
/// ToolCall kind events — no AssistantMessage kind — yet still carries usage.
pub async fn assistant_raw_lines(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<AssistantRawLine>> {
    let rows = sqlx::query(
        "SELECT oe.raw_event_id            AS raw_event_id,
                oe.session_id              AS session_id,
                MIN(oe.observed_at)        AS observed_at,
                json_extract(oe.payload,'$.model') AS model,
                CAST(re.payload AS TEXT)   AS raw
         FROM observed_event oe
         JOIN raw_event re ON oe.raw_event_id = re.raw_event_id
         WHERE oe.actor = 'assistant' AND oe.session_id = ?
         GROUP BY oe.raw_event_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_assistant_raw_line).collect())
}

pub async fn session_aggregate(pool: &SqlitePool, session_id: &str) -> Result<UsageAggregate> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS turns,
                COALESCE(SUM(input_tokens),0) AS input_tokens,
                COALESCE(SUM(cache_creation_input_tokens),0) AS cc,
                COALESCE(SUM(cache_read_input_tokens),0) AS cr,
                COALESCE(SUM(output_tokens),0) AS output_tokens
         FROM usage_facet WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    let by_model_rows = sqlx::query(
        "SELECT COALESCE(model,'unknown') AS model,
                COUNT(*) AS turns,
                COALESCE(SUM(output_tokens),0) AS output_tokens
         FROM usage_facet WHERE session_id = ?
         GROUP BY model ORDER BY turns DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(UsageAggregate {
        turns: row.get::<i64, _>("turns"),
        input_tokens: row.get::<i64, _>("input_tokens"),
        cache_creation_input_tokens: row.get::<i64, _>("cc"),
        cache_read_input_tokens: row.get::<i64, _>("cr"),
        output_tokens: row.get::<i64, _>("output_tokens"),
        by_model: by_model_rows.into_iter().map(map_model_usage).collect(),
    })
}

fn map_assistant_raw_line(r: sqlx::sqlite::SqliteRow) -> AssistantRawLine {
    AssistantRawLine {
        raw_event_id: r.get("raw_event_id"),
        session_id: r.get("session_id"),
        observed_at: r.get("observed_at"),
        model: r.get("model"),
        raw: r.get("raw"),
    }
}

fn map_model_usage(r: sqlx::sqlite::SqliteRow) -> ModelUsage {
    ModelUsage {
        model: r.get("model"),
        turns: r.get::<i64, _>("turns"),
        output_tokens: r.get::<i64, _>("output_tokens"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use sqlx::sqlite::SqlitePoolOptions;

    fn row(raw_event_id: &str, model: &str, input: i64, cc: i64, cr: i64, output: i64) -> UsageFacetRow {
        UsageFacetRow {
            raw_event_id: raw_event_id.into(),
            schema_version: "usage_facet.v1".into(),
            session_id: "sess_uf_test".into(),
            model: Some(model.into()),
            input_tokens: input,
            cache_creation_input_tokens: cc,
            cache_read_input_tokens: cr,
            output_tokens: output,
            observed_at: "2026-05-30T10:00:00Z".into(),
            parser_version: "usage_facet@v1".into(),
        }
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn roundtrip_and_aggregate() {
        let pool = pool().await;
        insert(&pool, &row("raw_001", "claude-opus-4-8", 2, 100, 5000, 300))
            .await
            .unwrap();
        insert(
            &pool,
            &row("raw_002", "claude-haiku-4-5-20251001", 3, 200, 6000, 400),
        )
        .await
        .unwrap();

        let agg = session_aggregate(&pool, "sess_uf_test").await.unwrap();
        assert_eq!(agg.turns, 2);
        assert_eq!(agg.input_tokens, 5);
        assert_eq!(agg.cache_creation_input_tokens, 300);
        assert_eq!(agg.cache_read_input_tokens, 11000);
        assert_eq!(agg.output_tokens, 700);

        assert_eq!(agg.by_model.len(), 2, "two distinct models -> two rows");
        let opus = agg
            .by_model
            .iter()
            .find(|m| m.model == "claude-opus-4-8")
            .expect("opus row present");
        assert_eq!(opus.turns, 1);
        assert_eq!(opus.output_tokens, 300);
        let haiku = agg
            .by_model
            .iter()
            .find(|m| m.model == "claude-haiku-4-5-20251001")
            .expect("haiku row present");
        assert_eq!(haiku.turns, 1);
        assert_eq!(haiku.output_tokens, 400);
    }

    #[tokio::test]
    async fn insert_or_replace_dedup() {
        let pool = pool().await;
        // First insert.
        insert(&pool, &row("raw_dup", "claude-opus-4-8", 2, 100, 5000, 300))
            .await
            .unwrap();
        // Same raw_event_id (PK) with different token values — must replace.
        insert(&pool, &row("raw_dup", "claude-opus-4-8", 9, 900, 9000, 999))
            .await
            .unwrap();

        let agg = session_aggregate(&pool, "sess_uf_test").await.unwrap();
        assert_eq!(agg.turns, 1, "INSERT OR REPLACE must deduplicate by PK");
        assert_eq!(agg.input_tokens, 9, "aggregate reflects the replaced row");
        assert_eq!(agg.cache_creation_input_tokens, 900);
        assert_eq!(agg.cache_read_input_tokens, 9000);
        assert_eq!(agg.output_tokens, 999);
    }
}
