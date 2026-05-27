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
    /// Slice-18: redacted state string ("redacted" | "not_redacted" | "not_applicable").
    /// Replaces the previous placeholder "unredacted".
    pub redaction_state: String,
    /// Slice-18: JSON-serialised RedactionManifest, or None for unparseable rows.
    pub redaction_manifest: Option<String>,
}

pub struct RawForEventRow {
    pub event_id: String,
    pub session_id: String,
    pub kind: String,
    pub raw_event_id: String,
    pub source_type: String,
    pub source_uri: String,
    pub source_line_no: i64,
    pub captured_at: String, // RFC3339 string straight from sqlite
    pub payload: Vec<u8>,
    pub observed_payload: String,
}

pub async fn get_for_event_id(pool: &SqlitePool, event_id: &str) -> Result<Option<RawForEventRow>> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT o.event_id        AS event_id, \
                o.session_id      AS session_id, \
                o.kind            AS kind, \
                o.payload         AS observed_payload, \
                r.raw_event_id    AS raw_event_id, \
                r.source_type     AS source_type, \
                r.source_uri      AS source_uri, \
                r.source_line_no  AS source_line_no, \
                r.captured_at     AS captured_at, \
                r.payload         AS payload \
         FROM observed_event o \
         JOIN raw_event r ON r.raw_event_id = o.raw_event_id \
         WHERE o.event_id = ?",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| RawForEventRow {
        event_id: r.get("event_id"),
        session_id: r.get("session_id"),
        kind: r.get("kind"),
        raw_event_id: r.get("raw_event_id"),
        source_type: r.get("source_type"),
        source_uri: r.get("source_uri"),
        source_line_no: r.get("source_line_no"),
        captured_at: r.get("captured_at"),
        payload: r.get("payload"),
        observed_payload: r.get("observed_payload"),
    }))
}

/// Returns true if a new row was inserted; false if the
/// `(source_uri, source_line_no, payload_sha256)` triple already existed.
pub async fn insert_dedup(pool: &SqlitePool, r: &NewRaw) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO raw_event(
            raw_event_id, ingest_run_id, source_type, source_uri,
            source_line_no, source_byte_offset, payload_sha256, payload,
            parse_error, captured_at, redaction_state, redaction_manifest)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(&r.redaction_state)
    .bind(&r.redaction_manifest)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
