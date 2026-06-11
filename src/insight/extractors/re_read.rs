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

        // Group Read tool_calls by (scope, file_path), then by read RANGE
        // (offset, limit). Dogfooding 2026-06-11:
        //  - Reading the SAME file at DIFFERENT ranges is pagination of a large
        //    file, NOT context-loss. Only a repeated SAME range counts as re-read
        //    (an absent offset/limit normalises to (-1,-1) = "whole-file read",
        //    so repeated whole-file reads still fire).
        //  - main vs subagent(sidechain) are separate contexts; merging them
        //    inflates the main session's signal with subagent reads. Scope by
        //    is_sidechain so each surfaces on its own (subagent re-reads are still
        //    measured, under scope="sidechain").
        type RangeMap = BTreeMap<(i64, i64), Vec<String>>;
        let mut groups: BTreeMap<(&'static str, String), RangeMap> = BTreeMap::new();

        for ev in view.events {
            if ev.kind != EventKind::ToolCall {
                continue;
            }
            if ev.tool_name.as_deref() != Some("Read") {
                continue;
            }
            // Real payload: {"content_ordinal": N, "tool_name": "Read", "input": {...}}
            // Fallback: {"tool_use": {"name": "Read", "input": {...}}}
            let input = ev
                .payload
                .pointer("/input")
                .or_else(|| ev.payload.pointer("/tool_use/input"));
            let Some(path) = input.and_then(|i| i.get("file_path")).and_then(|v| v.as_str())
            else {
                continue;
            };
            let offset = input
                .and_then(|i| i.get("offset"))
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            let limit = input
                .and_then(|i| i.get("limit"))
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            let scope = if ev.is_sidechain { "sidechain" } else { "main" };
            groups
                .entry((scope, path.to_string()))
                .or_default()
                .entry((offset, limit))
                .or_default()
                .push(ev.event_id.clone());
        }

        let mut out = Vec::new();
        for ((scope, path), ranges) in groups {
            // Fire when SOME range was read >= min_reads times. read_count is the
            // most-repeated range's count; evidence = the ids of every repeated range.
            let read_count = ranges.values().map(|v| v.len()).max().unwrap_or(0);
            if read_count < min_reads {
                continue;
            }
            let evidence_refs: Vec<String> = ranges
                .values()
                .filter(|v| v.len() >= min_reads)
                .flatten()
                .cloned()
                .collect();
            let summary = format!(
                "File {path} read {read_count} times at the same range ({scope}; re-read, context-loss signal)."
            );
            out.push(SignalCandidate {
                detector: "re_read",
                subkind: None,
                summary,
                evidence_refs,
                // Stable signal_id keyed by (scope, file_path) so accumulating reads
                // update one row instead of spawning a new signal each re-ingest, and
                // main vs sidechain stay distinct (dogfooding fix 2026-06-11).
                dedup_key: Some(format!("{scope}:{path}")),
                facts: json!({ "file_path": path, "read_count": read_count, "scope": scope }),
            });
        }
        out
    }

    fn manifest(&self) -> DetectorManifest {
        DetectorManifest {
            id: "re_read",
            intent: "같은 file_path를 같은 범위(offset,limit)로 min_reads회 이상 반복 Read (컨텍스트 망각 신호). 다른 범위 읽기(페이지네이션)는 제외하고, main/sidechain scope를 분리한다. spec §6.1/§10.5.",
            // Verified against detect(): reads ev.tool_name (Read filter),
            // ev.is_sidechain (scope), ev.payload /input/{file_path,offset,limit}.
            inputs: vec![
                "tool_call.tool_name(Read)",
                "tool_call.input.file_path",
                "tool_call.input.offset",
                "tool_call.input.limit",
                "observed_event.is_sidechain",
            ],
            rule: "tool_name==\"Read\" ToolCall을 (scope=main|sidechain, file_path)로 묶고 다시 (offset,limit) 범위로 나눠, 어떤 범위가 >= min_reads회 반복되면 (scope,file_path)당 1 signal 발화. offset/limit 없으면 (-1,-1)=전체읽기. 판단 없음(facts only).",
            output: "{file_path, read_count, scope}",
            // Verified: cfg.usize_param("re_read", "min_reads", MIN_READS_DEFAULT)
            config_keys: vec!["min_reads"],
            rationale: "spec §4.2 E그룹 · §6.1 시퀀스 · §10.5 1차 (실데이터로 임계값 잠금 예정)",
        }
    }
}
