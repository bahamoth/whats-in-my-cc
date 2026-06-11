//! `re_read` sequence detector (spec §6.1/§10.5): same file_path Read'd repeatedly.
//!
//! Deterministic; fires per path when Read tool_call count >= min_reads (config, default 2).
//! Facts only. evidence_refs = the Read tool_call event_ids for that path.
//!
//! Payload pointer: `/input/file_path` — verified against `src/ingest/mapping.rs:193`
//! where ToolCall payload is `{"content_ordinal": N, "tool_name": "Read", "input": {...}}`.
//! A fallback to `/tool_use/input/file_path` is also tried so synthetic test payloads
//! using that shape also work (matches the convention in extractor_risky_action tests).

use std::collections::BTreeMap;

use serde_json::json;

use crate::insight::config::DetectorConfig;
use crate::insight::extractor::Detector;
use crate::insight::manifest::DetectorManifest;
use crate::insight::types::SignalCandidate;
use crate::insight::view::SessionInsightView;
use crate::model::observed::EventKind;

/// Default minimum number of reads of the same file to fire.
const MIN_READS_DEFAULT: usize = 2;

pub struct ReRead;

impl Detector for ReRead {
    fn id(&self) -> &'static str {
        "re_read"
    }

    fn detect(
        &self,
        view: &SessionInsightView<'_>,
        cfg: &DetectorConfig,
    ) -> Vec<SignalCandidate> {
        let min_reads = cfg.usize_param("re_read", "min_reads", MIN_READS_DEFAULT);

        // Collect Read tool_call event_ids keyed by file_path.
        let mut by_path: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for ev in view.events {
            if ev.kind != EventKind::ToolCall {
                continue;
            }
            if ev.tool_name.as_deref() != Some("Read") {
                continue;
            }
            // Real payload: {"content_ordinal": N, "tool_name": "Read", "input": {"file_path": "..."}}
            // Fallback: {"tool_use": {"name": "Read", "input": {"file_path": "..."}}}
            let path = ev
                .payload
                .pointer("/input/file_path")
                .or_else(|| ev.payload.pointer("/tool_use/input/file_path"))
                .and_then(|v| v.as_str());

            if let Some(p) = path {
                by_path
                    .entry(p.to_string())
                    .or_default()
                    .push(ev.event_id.clone());
            }
        }

        let mut out = Vec::new();
        for (path, ids) in by_path {
            if ids.len() < min_reads {
                continue;
            }
            let read_count = ids.len();
            let summary = format!(
                "File {path} read {read_count} times (re-read, context-loss signal)."
            );
            out.push(SignalCandidate {
                detector: "re_read",
                subkind: None,
                summary,
                evidence_refs: ids,
                // re_read aggregates per file: keep a stable signal_id keyed by
                // file_path so accumulating reads update one row instead of
                // spawning a new signal each re-ingest (dogfooding fix 2026-06-11).
                dedup_key: Some(path.clone()),
                facts: json!({ "file_path": path, "read_count": read_count }),
            });
        }
        out
    }

    fn manifest(&self) -> DetectorManifest {
        DetectorManifest {
            id: "re_read",
            intent: "동일 file_path를 Read 도구로 min_reads회 이상 반복 읽음 (컨텍스트 망각 신호). spec §6.1/§10.5.",
            // Verified against detect(): reads ev.tool_name (Read filter),
            // ev.payload pointer /input/file_path (real mapping.rs shape).
            inputs: vec![
                "tool_call.tool_name(Read)",
                "tool_call.input.file_path",
            ],
            rule: "tool_name==\"Read\" ToolCall에서 같은 file_path 등장 횟수 >= min_reads이면 file_path당 1 signal 발화. 판단 없음(facts only).",
            output: "{file_path, read_count}",
            // Verified: cfg.usize_param("re_read", "min_reads", MIN_READS_DEFAULT)
            config_keys: vec!["min_reads"],
            rationale: "spec §4.2 E그룹 · §6.1 시퀀스 · §10.5 1차 (실데이터로 임계값 잠금 예정)",
        }
    }
}
