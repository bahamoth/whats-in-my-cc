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
    pub assistant_events: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
    pub by_model: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub model: String,
    pub assistant_events: i64,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
}

/// insight-redesign #6 — one row per session that has usage_facet rows.
/// Each metric is computed the same way `/v1/sessions/:id/usage` computes it,
/// so a session's value is directly comparable to the cross-session baseline.
#[derive(Debug, Clone)]
pub struct SessionMetrics {
    pub session_id: String,
    /// cache_read / (cache_read + cache_creation + input); None when denom 0.
    pub cache_hit_ratio: Option<f64>,
    /// input + cache_creation + output (cache_read is NOT billed).
    pub billed_tokens: i64,
    /// Number of usage_facet rows for this session (= assistant event count).
    pub assistant_events: i64,
    pub output_tokens: i64,
}

/// insight-redesign #6 — quantile triple for one baseline metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantiles {
    pub p25: f64,
    pub median: f64,
    pub p75: f64,
}

/// Compute p25 / median (p50) / p75 from an unsorted slice of values using the
/// "nearest-rank with linear interpolation" method (type-7, the default in R /
/// numpy.percentile). Returns None for an empty slice. SQLite has no MEDIAN(),
/// so this is computed in Rust over the per-session metric values.
pub fn median_p25_p75(values: &[f64]) -> Option<Quantiles> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let quantile = |p: f64| -> f64 {
        // type-7 (R/numpy default): rank h = (n-1)*p, linear interpolation.
        let n = v.len();
        if n == 1 {
            return v[0];
        }
        let h = (n as f64 - 1.0) * p;
        let lo = h.floor() as usize;
        let hi = h.ceil() as usize;
        let frac = h - lo as f64;
        v[lo] + frac * (v[hi] - v[lo])
    };
    Some(Quantiles {
        p25: quantile(0.25),
        median: quantile(0.5),
        p75: quantile(0.75),
    })
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
        "SELECT COUNT(*) AS assistant_events,
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
                COUNT(*) AS assistant_events,
                COALESCE(SUM(input_tokens),0) AS input_tokens,
                COALESCE(SUM(cache_creation_input_tokens),0) AS cc,
                COALESCE(SUM(cache_read_input_tokens),0) AS cr,
                COALESCE(SUM(output_tokens),0) AS output_tokens
         FROM usage_facet WHERE session_id = ?
         GROUP BY model ORDER BY assistant_events DESC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(UsageAggregate {
        assistant_events: row.get::<i64, _>("assistant_events"),
        input_tokens: row.get::<i64, _>("input_tokens"),
        cache_creation_input_tokens: row.get::<i64, _>("cc"),
        cache_read_input_tokens: row.get::<i64, _>("cr"),
        output_tokens: row.get::<i64, _>("output_tokens"),
        by_model: by_model_rows.into_iter().map(map_model_usage).collect(),
    })
}

/// S8 (UX 재설계) — per-turn token sums for a session, keyed by `turn_id`.
/// usage_facet has no turn_id of its own, so it is joined to observed_event on
/// `raw_event_id` (the assistant line that carried `message.usage`); the turn
/// is observed_event.turn_id (= the transcript `promptId`). Turns with no
/// correlated usage row are simply absent from the map.
pub async fn tokens_by_turn(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<std::collections::HashMap<String, (i64, i64, i64, i64)>> {
    let rows = sqlx::query(
        "SELECT oe.turn_id AS turn_id,
                COALESCE(SUM(uf.input_tokens),0) AS input_tokens,
                COALESCE(SUM(uf.cache_creation_input_tokens),0) AS cc,
                COALESCE(SUM(uf.cache_read_input_tokens),0) AS cr,
                COALESCE(SUM(uf.output_tokens),0) AS output_tokens
         FROM usage_facet uf
         JOIN observed_event oe ON oe.raw_event_id = uf.raw_event_id
         WHERE uf.session_id = ? AND oe.turn_id IS NOT NULL AND oe.turn_id != ''
         GROUP BY oe.turn_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        out.insert(
            r.get::<String, _>("turn_id"),
            (
                r.get::<i64, _>("input_tokens"),
                r.get::<i64, _>("cc"),
                r.get::<i64, _>("cr"),
                r.get::<i64, _>("output_tokens"),
            ),
        );
    }
    Ok(out)
}

/// insight-redesign #6 — per-session metric rows for the cross-session
/// baseline. One row per session that has at least one usage_facet row.
/// billed_tokens = input + cache_creation + output (cache_read NOT billed).
/// cache_hit_ratio = cache_read / (cache_read + cache_creation + input);
/// NULL when the denominator is 0 (mirrors `/v1/sessions/:id/usage`).
pub async fn per_session_metrics(pool: &SqlitePool) -> Result<Vec<SessionMetrics>> {
    let rows = sqlx::query(
        "SELECT session_id,
                COUNT(*) AS assistant_events,
                COALESCE(SUM(input_tokens),0)
                  + COALESCE(SUM(cache_creation_input_tokens),0)
                  + COALESCE(SUM(output_tokens),0)            AS billed_tokens,
                COALESCE(SUM(output_tokens),0)                AS output_tokens,
                CASE
                  WHEN (COALESCE(SUM(cache_read_input_tokens),0)
                        + COALESCE(SUM(cache_creation_input_tokens),0)
                        + COALESCE(SUM(input_tokens),0)) > 0
                  THEN CAST(COALESCE(SUM(cache_read_input_tokens),0) AS REAL)
                       / (COALESCE(SUM(cache_read_input_tokens),0)
                          + COALESCE(SUM(cache_creation_input_tokens),0)
                          + COALESCE(SUM(input_tokens),0))
                  ELSE NULL
                END                                           AS cache_hit_ratio
         FROM usage_facet
         GROUP BY session_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(map_session_metrics).collect())
}

fn map_session_metrics(r: sqlx::sqlite::SqliteRow) -> SessionMetrics {
    SessionMetrics {
        session_id: r.get("session_id"),
        cache_hit_ratio: r.get::<Option<f64>, _>("cache_hit_ratio"),
        billed_tokens: r.get::<i64, _>("billed_tokens"),
        assistant_events: r.get::<i64, _>("assistant_events"),
        output_tokens: r.get::<i64, _>("output_tokens"),
    }
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
        assistant_events: r.get::<i64, _>("assistant_events"),
        input_tokens: r.get::<i64, _>("input_tokens"),
        cache_creation_input_tokens: r.get::<i64, _>("cc"),
        cache_read_input_tokens: r.get::<i64, _>("cr"),
        output_tokens: r.get::<i64, _>("output_tokens"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use sqlx::sqlite::SqlitePoolOptions;

    fn row(
        raw_event_id: &str,
        model: &str,
        input: i64,
        cc: i64,
        cr: i64,
        output: i64,
    ) -> UsageFacetRow {
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
        assert_eq!(agg.assistant_events, 2);
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
        assert_eq!(opus.assistant_events, 1);
        assert_eq!(opus.output_tokens, 300);
        assert_eq!(opus.input_tokens, 2, "per-model input sum");
        assert_eq!(
            opus.cache_creation_input_tokens, 100,
            "per-model cache_creation sum"
        );
        assert_eq!(
            opus.cache_read_input_tokens, 5000,
            "per-model cache_read sum"
        );
        let haiku = agg
            .by_model
            .iter()
            .find(|m| m.model == "claude-haiku-4-5-20251001")
            .expect("haiku row present");
        assert_eq!(haiku.assistant_events, 1);
        assert_eq!(haiku.output_tokens, 400);
        assert_eq!(haiku.input_tokens, 3);
        assert_eq!(haiku.cache_creation_input_tokens, 200);
        assert_eq!(haiku.cache_read_input_tokens, 6000);
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
        assert_eq!(
            agg.assistant_events, 1,
            "INSERT OR REPLACE must deduplicate by PK"
        );
        assert_eq!(agg.input_tokens, 9, "aggregate reflects the replaced row");
        assert_eq!(agg.cache_creation_input_tokens, 900);
        assert_eq!(agg.cache_read_input_tokens, 9000);
        assert_eq!(agg.output_tokens, 999);
    }

    #[test]
    fn quantiles_of_empty_is_none() {
        assert!(median_p25_p75(&[]).is_none());
    }

    #[test]
    fn quantiles_single_value() {
        let q = median_p25_p75(&[7.0]).unwrap();
        assert_eq!(q.median, 7.0);
        assert_eq!(q.p25, 7.0);
        assert_eq!(q.p75, 7.0);
    }

    #[test]
    fn median_odd_count_is_middle() {
        // sorted: 1,3,5,7,9 -> median index 2 -> 5.0
        let q = median_p25_p75(&[5.0, 1.0, 9.0, 3.0, 7.0]).unwrap();
        assert_eq!(q.median, 5.0);
    }

    #[test]
    fn median_even_count_interpolates_midpoint() {
        // sorted: 2,4,6,8 -> median between 4 and 6 -> 5.0
        let q = median_p25_p75(&[8.0, 2.0, 6.0, 4.0]).unwrap();
        assert_eq!(q.median, 5.0);
    }

    #[test]
    fn p25_p75_type7_interpolation() {
        // sorted: 10,20,30,40 (n=4). type-7 rank h = (n-1)*p.
        // p25: h = 3*0.25 = 0.75 -> 10 + 0.75*(20-10) = 17.5
        // p75: h = 3*0.75 = 2.25 -> 30 + 0.25*(40-30) = 32.5
        let q = median_p25_p75(&[40.0, 10.0, 30.0, 20.0]).unwrap();
        assert_eq!(q.p25, 17.5);
        assert_eq!(q.median, 25.0);
        assert_eq!(q.p75, 32.5);
    }

    #[tokio::test]
    async fn per_session_metrics_one_row_per_session() {
        let pool = pool().await;
        // Session A (the shared sess_uf_test): two opus/haiku assistant_events.
        insert(&pool, &row("raw_a1", "claude-opus-4-8", 2, 100, 5000, 300))
            .await
            .unwrap();
        insert(
            &pool,
            &row("raw_a2", "claude-haiku-4-5-20251001", 3, 200, 6000, 400),
        )
        .await
        .unwrap();
        // Session B: one assistant_event, distinct session_id.
        let mut b = row("raw_b1", "claude-opus-4-8", 10, 0, 0, 50);
        b.session_id = "sess_uf_other".into();
        insert(&pool, &b).await.unwrap();

        let mut metrics = per_session_metrics(&pool).await.unwrap();
        metrics.sort_by(|x, y| x.session_id.cmp(&y.session_id));
        assert_eq!(metrics.len(), 2, "one row per session with usage rows");

        let a = &metrics[0];
        assert_eq!(a.session_id, "sess_uf_other");
        // Session B denom = 0+0+10 = 10, cache_read 0 -> ratio 0.0.
        assert_eq!(a.cache_hit_ratio, Some(0.0));
        assert_eq!(a.billed_tokens, 10 + 50);
        assert_eq!(a.assistant_events, 1);
        assert_eq!(a.output_tokens, 50);

        let s = &metrics[1];
        assert_eq!(s.session_id, "sess_uf_test");
        // Session A: input 5, cc 300, cr 11000 -> denom 11305, ratio 11000/11305.
        let ratio = s.cache_hit_ratio.unwrap();
        assert!((ratio - 11000.0 / 11305.0).abs() < 1e-9);
        assert_eq!(s.billed_tokens, 5 + 300 + 700);
        assert_eq!(s.assistant_events, 2);
        assert_eq!(s.output_tokens, 700);
    }
}
