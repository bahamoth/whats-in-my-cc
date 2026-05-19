use serde::Serialize;

pub const SCHEMA_VERSION: &str = "0.1.0";
pub const PARSER_VERSION_TRANSCRIPT: &str = "transcript@0.1.0";
pub const COLLECTION_PROFILE: &str = "local_transcript_slice1";

#[derive(Debug, Serialize)]
pub struct ResponseMeta {
    pub schema_version: &'static str,
    pub collection_profile: &'static str,
    pub redaction_policy: Option<&'static str>, // always None in slice-1
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub next_cursor: Option<String>,            // always None in slice-1
}

impl ResponseMeta {
    pub fn now() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            collection_profile: COLLECTION_PROFILE,
            redaction_policy: None,
            generated_at: chrono::Utc::now(),
            next_cursor: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub meta: ResponseMeta,
    pub data: T,
}
