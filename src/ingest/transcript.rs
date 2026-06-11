use chrono::{DateTime, Utc};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;

use crate::error::WimccError;

pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LineMeta {
    pub source_uri: PathBuf,
    pub line_no: u64,
    pub byte_offset: u64,
    pub raw: Vec<u8>, // original line bytes (without newline)
}

#[derive(Debug)]
pub enum ParsedRecord {
    User(UserRecord),
    Assistant(AssistantRecord),
    Attachment(AttachmentRecord),
    SystemMsg(SystemRecord),
    PermissionMode(PermissionModeRecord),
    LastPrompt(LastPromptRecord),
    FileHistorySnapshot(FileHistorySnapshotRecord),
    Unknown(Value),
}

impl ParsedRecord {
    /// Return the `sessionId` field on every variant that carries one.
    /// `Unknown` and `FileHistorySnapshot` (no embedded sessionId) return
    /// `None`. Used by `ingest::store::ingest_file` so it can mark a session as
    /// touched even when raw_event was a dedup no-op — needed so the insight
    /// pipeline re-runs for previously-ingested sessions.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            ParsedRecord::User(r) => Some(&r.session_id),
            ParsedRecord::Assistant(r) => Some(&r.session_id),
            ParsedRecord::Attachment(r) => Some(&r.session_id),
            ParsedRecord::SystemMsg(r) => Some(&r.session_id),
            ParsedRecord::PermissionMode(r) => Some(&r.session_id),
            ParsedRecord::LastPrompt(r) => Some(&r.session_id),
            ParsedRecord::FileHistorySnapshot(_) | ParsedRecord::Unknown(_) => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UserRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "userType")]
    pub user_type: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "isSidechain")]
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(rename = "agentId")]
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(rename = "isMeta")]
    #[serde(default)]
    pub is_meta: bool,
    #[serde(rename = "promptId")]
    pub prompt_id: Option<String>,
    #[serde(rename = "sourceToolAssistantUUID")]
    pub source_tool_assistant_uuid: Option<String>,
    #[serde(rename = "sourceToolUseID")]
    pub source_tool_use_id: Option<String>,
    pub message: Value,
    /// Slice-10a — top-level `toolUseResult` envelope (Edit/Write outputs
    /// carry `filePath`, `structuredPatch`, `userModified`, `oldString`,
    /// `newString` here). Optional because not every `user` record is a
    /// tool_result.
    #[serde(rename = "toolUseResult")]
    #[serde(default)]
    pub tool_use_result: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "userType")]
    pub user_type: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "isSidechain")]
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(rename = "agentId")]
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    pub message: Value,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "userType")]
    pub user_type: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "isSidechain")]
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(rename = "agentId")]
    #[serde(default)]
    pub agent_id: Option<String>,
    pub attachment: Value,
}

#[derive(Debug, Deserialize)]
pub struct SystemRecord {
    pub uuid: String,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub subtype: Option<String>,
    #[serde(rename = "toolUseID")]
    pub tool_use_id: Option<String>,
    #[serde(flatten)]
    pub rest: Value,
}

#[derive(Debug, Deserialize)]
pub struct PermissionModeRecord {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "permissionMode")]
    pub permission_mode: String,
}

#[derive(Debug, Deserialize)]
pub struct LastPromptRecord {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "leafUuid")]
    pub leaf_uuid: String,
}

#[derive(Debug, Deserialize)]
pub struct FileHistorySnapshotRecord {
    #[serde(rename = "messageId")]
    pub message_id: String,
    #[serde(rename = "isSnapshotUpdate")]
    pub is_snapshot_update: bool,
    pub snapshot: Value,
}

pub async fn stream_file(
    path: &Path,
) -> std::io::Result<impl Stream<Item = Result<(LineMeta, ParsedRecord), WimccError>>> {
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::with_capacity(64 * 1024, file);
    let source_uri: PathBuf = path.to_path_buf();
    let lines = reader.lines();
    Ok(futures::stream::unfold(
        (lines, 0u64, 0u64, source_uri),
        |(mut lines, line_no, byte_offset, src)| async move {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let new_line_no = line_no + 1;
                    let raw_bytes = line.as_bytes().to_vec();
                    let new_offset = byte_offset + raw_bytes.len() as u64 + 1;
                    let mut payload = raw_bytes.clone();
                    if payload.len() > MAX_LINE_BYTES {
                        payload.truncate(MAX_LINE_BYTES);
                    }
                    let meta = LineMeta {
                        source_uri: src.clone(),
                        line_no: new_line_no,
                        byte_offset,
                        raw: payload,
                    };
                    let value: Result<Value, _> = serde_json::from_str(&line);
                    let result = match value {
                        Ok(v) => dispatch(v).map(|rec| (meta.clone(), rec)).map_err(|e| {
                            WimccError::ParseLine {
                                source_uri: src.display().to_string(),
                                line_no: new_line_no,
                                message: e,
                            }
                        }),
                        Err(e) => Err(WimccError::ParseLine {
                            source_uri: src.display().to_string(),
                            line_no: new_line_no,
                            message: e.to_string(),
                        }),
                    };
                    Some((result, (lines, new_line_no, new_offset, src)))
                }
                Ok(None) => None,
                Err(e) => Some((
                    Err(WimccError::Io {
                        path: src.clone(),
                        source: e,
                    }),
                    (lines, line_no, byte_offset, src),
                )),
            }
        },
    ))
}

fn dispatch(value: Value) -> Result<ParsedRecord, String> {
    let tag = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let rec = match tag {
        "user" => ParsedRecord::User(serde_json::from_value(value).map_err(|e| e.to_string())?),
        "assistant" => {
            ParsedRecord::Assistant(serde_json::from_value(value).map_err(|e| e.to_string())?)
        }
        "attachment" => {
            ParsedRecord::Attachment(serde_json::from_value(value).map_err(|e| e.to_string())?)
        }
        "system" => {
            ParsedRecord::SystemMsg(serde_json::from_value(value).map_err(|e| e.to_string())?)
        }
        "permission-mode" => {
            ParsedRecord::PermissionMode(serde_json::from_value(value).map_err(|e| e.to_string())?)
        }
        "last-prompt" => {
            ParsedRecord::LastPrompt(serde_json::from_value(value).map_err(|e| e.to_string())?)
        }
        "file-history-snapshot" => ParsedRecord::FileHistorySnapshot(
            serde_json::from_value(value).map_err(|e| e.to_string())?,
        ),
        _ => ParsedRecord::Unknown(value),
    };
    Ok(rec)
}
