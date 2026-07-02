use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "0.5.0";
pub const PARSER_VERSION_TRANSCRIPT: &str = "transcript@0.1.0";
pub const PARSER_VERSION_OTEL: &str = "otel@0.1.0";
pub const PARSER_VERSION_OTEL_METRICS: &str = "otel-metrics@0.5";
pub const PARSER_VERSION_OTEL_LOGS: &str = "otel-logs@0.5";
// PARSER_VERSION_HOOK / PARSER_VERSION_FILE_GIT는 2026-07-03 제거 — hook stdin
// collector(2026-06-19 폐지)·git poller(slice-10a에서 diff_hunk로 대체)의 잔재로,
// 참조하는 ingest 경로가 없었다. hook 이벤트는 transcript attachment에서 파생된다.
pub const COLLECTION_PROFILE: &str = "local_transcript_slice1";

pub const SOURCE_TYPE_OTEL_METRICS: &str = "otel-metrics";
pub const SOURCE_TYPE_OTEL_LOGS: &str = "otel-logs";

/// Slice-18 — per-response redaction policy declaration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RedactionPolicy {
    pub applied: bool,
    pub level: &'static str,
}

impl RedactionPolicy {
    pub fn standard() -> Self {
        Self {
            applied: true,
            level: "standard",
        }
    }
}

/// Slice-18 — aggregate redaction summary for a response's event set.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RedactionSummary {
    pub total_items_redacted: u32,
    pub rules_seen: Vec<String>,
    pub any_unredacted_sensitive: bool,
}

#[derive(Debug, Serialize)]
pub struct ResponseMeta {
    pub schema_version: &'static str,
    pub collection_profile: &'static str,
    /// Slice-18: always `RedactionPolicy::standard()`. `#[serde(default)]`
    /// ensures prior clients parsing `meta` don't break on the new field.
    #[serde(default)]
    pub redaction_policy: RedactionPolicy,
    /// Slice-18: aggregate summary; None when no raw events are in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction_summary: Option<RedactionSummary>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl ResponseMeta {
    pub fn now() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            collection_profile: COLLECTION_PROFILE,
            redaction_policy: RedactionPolicy::standard(),
            redaction_summary: None,
            generated_at: chrono::Utc::now(),
        }
    }

    pub fn with_summary(mut self, summary: RedactionSummary) -> Self {
        self.redaction_summary = Some(summary);
        self
    }
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub meta: ResponseMeta,
    pub data: T,
}
