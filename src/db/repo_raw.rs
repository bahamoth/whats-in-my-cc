use crate::error::Result;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

pub struct NewRaw {
    pub raw_event_id: String,
    pub ingest_run_id: String,
    pub source_type: String,
    pub source_uri: String,
    pub source_line_no: i64,
    pub source_byte_offset: i64,
    pub payload_sha256: String,
    pub payload: Vec<u8>,
    pub parse_error: Option<String>,
    pub captured_at: DateTime<Utc>,
}

/// Returns true if a new row was inserted; false if the
/// `(source_uri, source_line_no, payload_sha256)` triple already existed.
pub async fn insert_dedup(pool: &SqlitePool, r: &NewRaw) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO raw_event(
            raw_event_id, ingest_run_id, source_type, source_uri,
            source_line_no, source_byte_offset, payload_sha256, payload,
            parse_error, captured_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(source_uri, source_line_no, payload_sha256) DO NOTHING",
    )
    .bind(&r.raw_event_id)
    .bind(&r.ingest_run_id)
    .bind(&r.source_type)
    .bind(&r.source_uri)
    .bind(r.source_line_no)
    .bind(r.source_byte_offset)
    .bind(&r.payload_sha256)
    .bind(&r.payload)
    .bind(&r.parse_error)
    .bind(r.captured_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
