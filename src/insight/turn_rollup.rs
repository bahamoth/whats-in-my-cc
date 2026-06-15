//! Dogfood 2026-06-12 (§3-2) — turn-level deterministic rollup.
//!
//! Pure aggregation over a session's conversation events: per-user-turn tool
//! histogram + edited files, and cross-turn file churn. This is the exact
//! aggregation the 2026-06-12 dogfooding analysis had to hand-roll in Python;
//! shipping it as an API surface makes session retrospects scriptless.
//!
//! Judgment stays out by design (deterministic-measurement / LLM-judgment
//! split): counts, ids and redacted excerpts only — no severity, no "rework"
//! classification.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::model::observed::{EventKind, ObservedEvent};

/// Tools whose `input.file_path` mutates a file. NotebookEdit included for
/// completeness even though transcripts mapped so far only carry Edit/Write.
const FILE_EDIT_TOOLS: &[&str] = &["Edit", "Write", "NotebookEdit"];

/// Max excerpt length in *chars* (multibyte-safe — payloads are Korean-heavy).
const EXCERPT_CHARS: usize = 240;

#[derive(Debug, Serialize)]
pub struct TurnUserMessage {
    pub event_id: String,
    pub observed_at: String,
    pub is_meta: bool,
    /// First EXCERPT_CHARS chars of the message text. Payloads are redacted at
    /// ingest (rule pack v1), so this is already-masked text.
    pub excerpt: String,
}

/// S8 (UX 재설계) — per-turn token sums (from `usage_facet`, joined by
/// `turn_id`). Drives the KPI strip's intra-session sparklines. `None` when no
/// usage rows are correlated to the turn (e.g. tool-only assistant turns, or
/// the MCP path that does not attach token sums).
#[derive(Debug, Serialize)]
pub struct TurnTokens {
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Serialize)]
pub struct TurnRollup {
    pub turn_id: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    /// First non-meta user_message of the turn; None for turns observed
    /// without one (e.g. sidechain-only or truncated ingests).
    pub user_message: Option<TurnUserMessage>,
    pub tool_call_total: i64,
    pub tool_histogram: BTreeMap<String, i64>,
    /// verb.object 태그별 tool_call 수 (event_tags 분류기 — tagged만 카운트).
    /// raw tool 이름(tool_histogram)과 달리 작업의 의미 어휘로 본 구성이다.
    pub tag_histogram: BTreeMap<String, i64>,
    /// Distinct file paths touched by Edit/Write/NotebookEdit in this turn,
    /// in first-touch order.
    pub files_edited: Vec<String>,
    /// S8 — per-turn token sums; attached by the HTTP handler after rollup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TurnTokens>,
}

#[derive(Debug, Serialize)]
pub struct FileChurn {
    pub file_path: String,
    /// Number of distinct turns that edited this file — the raw material for
    /// a future `re_edit_churn` Signal 규칙 (kept as a count, not a judgment).
    pub turn_count: i64,
    pub edit_count: i64,
}

#[derive(Debug, Serialize)]
pub struct TurnRollupResponse {
    pub session_id: String,
    pub turns: Vec<TurnRollup>,
    pub file_churn: Vec<FileChurn>,
}

fn excerpt_of(payload: &serde_json::Value) -> String {
    // user_message payload shapes per mapping.rs: {"text": ...} (content array
    // text item) or {"content": ...} (string-content branch).
    let text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("content").and_then(|v| v.as_str()))
        .unwrap_or("");
    text.chars().take(EXCERPT_CHARS).collect()
}

/// Aggregate conversation events (chronological order expected) into turns.
/// Events without a `turn_id` are skipped — bulk telemetry carries none.
pub fn rollup(session_id: &str, events: &[ObservedEvent]) -> TurnRollupResponse {
    struct Acc {
        first: String,
        last: String,
        user_message: Option<TurnUserMessage>,
        tool_call_total: i64,
        histogram: BTreeMap<String, i64>,
        tags: BTreeMap<String, i64>,
        files: Vec<String>,
        order: usize,
    }
    let mut turns: BTreeMap<String, Acc> = BTreeMap::new();
    let mut next_order = 0usize;

    for ev in events {
        let Some(turn_id) = ev.turn_id.as_deref().filter(|t| !t.is_empty()) else {
            continue;
        };
        let ts = ev.observed_at.to_rfc3339();
        let acc = turns.entry(turn_id.to_string()).or_insert_with(|| {
            let order = next_order;
            next_order += 1;
            Acc {
                first: ts.clone(),
                last: ts.clone(),
                user_message: None,
                tool_call_total: 0,
                histogram: BTreeMap::new(),
                tags: BTreeMap::new(),
                files: Vec::new(),
                order,
            }
        });
        if ts < acc.first {
            acc.first = ts.clone();
        }
        if ts > acc.last {
            acc.last = ts.clone();
        }
        match ev.kind {
            EventKind::UserMessage => {
                if acc.user_message.is_none() && !ev.is_meta {
                    acc.user_message = Some(TurnUserMessage {
                        event_id: ev.event_id.clone(),
                        observed_at: ts.clone(),
                        is_meta: ev.is_meta,
                        excerpt: excerpt_of(&ev.payload),
                    });
                }
            }
            EventKind::ToolCall => {
                acc.tool_call_total += 1;
                let name = ev
                    .tool_name
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .unwrap_or("unknown");
                *acc.histogram.entry(name.to_string()).or_insert(0) += 1;
                if let Some(tag) = crate::insight::event_tags::classify_tool_call(
                    ev.tool_name.as_deref(),
                    &ev.payload,
                )
                .value
                {
                    *acc.tags.entry(tag.to_string()).or_insert(0) += 1;
                }
                if FILE_EDIT_TOOLS.contains(&name) {
                    if let Some(fp) = ev
                        .payload
                        .pointer("/input/file_path")
                        .and_then(|v| v.as_str())
                    {
                        if !acc.files.iter().any(|f| f == fp) {
                            acc.files.push(fp.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Cross-turn file churn (distinct-turn + total-edit counts).
    let mut churn: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for ev in events {
        if ev.kind != EventKind::ToolCall {
            continue;
        }
        if ev.turn_id.as_deref().filter(|t| !t.is_empty()).is_none() {
            continue;
        }
        let name = ev.tool_name.as_deref().unwrap_or("");
        if !FILE_EDIT_TOOLS.contains(&name) {
            continue;
        }
        if let Some(fp) = ev
            .payload
            .pointer("/input/file_path")
            .and_then(|v| v.as_str())
        {
            let entry = churn.entry(fp.to_string()).or_insert((0, 0));
            entry.1 += 1;
        }
    }
    for (fp, counts) in churn.iter_mut() {
        counts.0 = turns
            .values()
            .filter(|a| a.files.iter().any(|f| f == fp))
            .count() as i64;
    }

    let mut ordered: Vec<(String, Acc)> = turns.into_iter().collect();
    ordered.sort_by_key(|(_, a)| a.order);
    let turns_out: Vec<TurnRollup> = ordered
        .into_iter()
        .map(|(turn_id, a)| TurnRollup {
            turn_id,
            first_observed_at: a.first,
            last_observed_at: a.last,
            user_message: a.user_message,
            tool_call_total: a.tool_call_total,
            tool_histogram: a.histogram,
            tag_histogram: a.tags,
            files_edited: a.files,
            tokens: None,
        })
        .collect();

    let mut churn_out: Vec<FileChurn> = churn
        .into_iter()
        .map(|(file_path, (turn_count, edit_count))| FileChurn {
            file_path,
            turn_count,
            edit_count,
        })
        .collect();
    churn_out.sort_by(|x, y| {
        y.edit_count
            .cmp(&x.edit_count)
            .then_with(|| x.file_path.cmp(&y.file_path))
    });

    TurnRollupResponse {
        session_id: session_id.to_string(),
        turns: turns_out,
        file_churn: churn_out,
    }
}
