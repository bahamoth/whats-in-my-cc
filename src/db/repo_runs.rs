use crate::error::Result;
use crate::ids::MonotonicUlidGen;
use sqlx::SqlitePool;

pub async fn start(pool: &SqlitePool) -> Result<String> {
    let run_id = MonotonicUlidGen::new().next();
    sqlx::query("INSERT INTO ingest_run(run_id, started_at, status) VALUES(?, ?, 'running')")
        .bind(&run_id).bind(chrono::Utc::now().to_rfc3339())
        .execute(pool).await?;
    Ok(run_id)
}

pub async fn finish(pool: &SqlitePool, run_id: &str, status: &str, stats: serde_json::Value) -> Result<()> {
    sqlx::query("UPDATE ingest_run SET finished_at=?, status=?, stats=? WHERE run_id=?")
        .bind(chrono::Utc::now().to_rfc3339()).bind(status).bind(stats.to_string()).bind(run_id)
        .execute(pool).await?;
    Ok(())
}
