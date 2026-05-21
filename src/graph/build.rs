use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use crate::db::{repo_graph, repo_observed};
use crate::error::Result;
use crate::ids::{derive_edge_id, derive_node_id};
use crate::model::graph::{GraphEdge, GraphNode};
use crate::model::meta::SCHEMA_VERSION;
use crate::model::observed::{EventKind, ObservedEvent};

pub async fn rebuild_session(pool: &SqlitePool, session_id: &str) -> Result<(usize, usize)> {
    let evs = repo_observed::list_session(pool, session_id, 100_000).await?;
    let (nodes, edges) = compute(session_id, &evs);
    repo_graph::delete_session(pool, session_id).await?;
    let n = nodes.len();
    let e = edges.len();
    repo_graph::insert_nodes_edges(pool, &nodes, &edges).await?;
    Ok((n, e))
}

pub fn compute(session_id: &str, events: &[ObservedEvent]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes: Vec<GraphNode> = Vec::new();
    // node_id -> index in `nodes` for deduplication
    let mut node_index_by_id: HashMap<String, usize> = HashMap::new();
    // event_uuid -> node_id (last writer wins; see below for merge fix-up)
    let mut by_event_uuid: HashMap<String, String> = HashMap::new();
    // tool_use_id -> index in `nodes` for tool_call nodes
    let mut tool_call_node: HashMap<String, usize> = HashMap::new();
    // tool_use_id -> (event_uuid, node_id) for tool_result nodes before merge
    let mut tool_result_info: HashMap<String, (Option<String>, String)> = HashMap::new();

    // 1. Node materialization
    for e in events {
        let (kind, merge_keys) = match e.kind {
            EventKind::UserMessage => (
                "user_message",
                json!({"session_id": session_id, "event_uuid": e.event_uuid}),
            ),
            EventKind::AssistantMessage => (
                "assistant_message",
                json!({"session_id": session_id, "event_uuid": e.event_uuid}),
            ),
            EventKind::ToolCall => (
                "tool_call",
                json!({"session_id": session_id, "tool_use_id": e.tool_use_id}),
            ),
            EventKind::ToolResult => (
                "tool_result",
                json!({"session_id": session_id, "tool_use_id": e.tool_use_id}),
            ),
            EventKind::HookEvent => {
                // External hooks (slice-4, parser_version starts with "hook@") carry
                // hook_event_name as their subkind and an optional tool_use_id.  These
                // are the correlation keys for cross-session dedup.  Transcript-internal
                // hook attachments (slice-1) instead key by event_uuid because they
                // arrive with no hook_event_name distinction.
                if e.parser_version.starts_with("hook@") {
                    (
                        "hook_event",
                        json!({
                            "session_id":      session_id,
                            "hook_event_name": e.subkind,
                            "tool_use_id":     e.tool_use_id,
                        }),
                    )
                } else {
                    (
                        "hook_event",
                        json!({"session_id": session_id, "event_uuid": e.event_uuid}),
                    )
                }
            }
            EventKind::OtelSpan => (
                "otel_span",
                json!({
                    "session_id": session_id,
                    "trace_id":   e.trace_id,
                    "span_id":    e.span_id,
                }),
            ),
            EventKind::FileEvent => (
                "file_event",
                json!({
                    "session_id":  session_id,
                    "file_path":   e.payload.pointer("/file/path"),
                    "change_type": e.payload.pointer("/file/change_type"),
                }),
            ),
            EventKind::GitCommit => (
                "git_commit",
                json!({
                    "session_id": session_id,
                    "sha":        e.payload.pointer("/git/sha"),
                }),
            ),
            EventKind::DiffHunk => (
                "diff_hunk",
                json!({
                    "session_id":   session_id,
                    "diff_hunk_id": e.payload.pointer("/hunk/diff_hunk_id"),
                }),
            ),
            EventKind::MetricSample => (
                "metric_sample",
                json!({
                    "session_id":      session_id,
                    "instrument_name": e.payload.get("instrument_name"),
                    "time_unix_nano":  e.payload.get("time_unix_nano"),
                    // event_id is already deterministic-by-(resource, instrument, time, attrs)
                    // — include it so distinct data points with same (instrument, time) but
                    // different attributes do not collapse onto the same graph node.
                    "event_id":        e.event_id,
                }),
            ),
            EventKind::LogRecord => (
                "log_record",
                json!({
                    "session_id":     session_id,
                    "time_unix_nano": e.payload.get("time_unix_nano"),
                    "event_name":     e.payload.get("event_name"),
                    "event_id":       e.event_id,
                }),
            ),
            // attachment_meta, session_state, file_history_snapshot, thinking,
            // system_summary, unknown — not promoted to graph nodes
            _ => continue,
        };
        let mk_string = merge_keys.to_string();
        let keys_for_hash = canonical_pairs(&merge_keys);
        let node_id = derive_node_id(
            kind,
            &keys_for_hash
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
        );

        // Deduplicate: if a node with this node_id already exists (e.g., multiple
        // text content elements in the same JSONL message share the same event_uuid),
        // accumulate the event_id onto the existing node instead of pushing a second
        // node with an identical node_id (which would cause a UNIQUE constraint
        // violation on insert).  The first event's payload is kept; additional
        // content is recoverable via observed_event lookup by source_event_ids.
        if let Some(&existing_idx) = node_index_by_id.get(&node_id) {
            nodes[existing_idx]
                .source_event_ids
                .push(e.event_id.clone());
            // Still update by_event_uuid so edge resolution stays correct.
            if let Some(uuid) = &e.event_uuid {
                by_event_uuid.insert(uuid.clone(), node_id.clone());
            }
            continue;
        }

        // Track event_uuid → node_id; tool_call takes priority over assistant_message
        // for the same uuid so that parent_uuid lookups land on the call node when
        // the reply is a tool_result.  We overwrite unconditionally here and fix up
        // after the merge step.
        if let Some(uuid) = &e.event_uuid {
            by_event_uuid.insert(uuid.clone(), node_id.clone());
        }

        if kind == "tool_call" {
            if let Some(tid) = &e.tool_use_id {
                tool_call_node.insert(tid.clone(), nodes.len());
            }
        }
        if kind == "tool_result" {
            if let Some(tid) = &e.tool_use_id {
                tool_result_info.insert(tid.clone(), (e.event_uuid.clone(), node_id.clone()));
            }
        }

        node_index_by_id.insert(node_id.clone(), nodes.len());
        nodes.push(GraphNode {
            node_id,
            schema_version: SCHEMA_VERSION.into(),
            session_id: session_id.into(),
            node_kind: kind.into(),
            started_at: e.observed_at,
            ended_at: None,
            merge_keys: serde_json::from_str(&mk_string).unwrap_or(merge_keys),
            source_event_ids: vec![e.event_id.clone()],
            source_uris: vec![],
            payload: e.payload.clone(),
        });
    }

    // 2. Merge tool_result payload into matching tool_call node.
    //    Collect all mutations first to satisfy the borrow checker, then apply.
    //    After merge the result node is removed; we update by_event_uuid so that
    //    any event whose parent_uuid matched the result's event_uuid is re-pointed
    //    to the (surviving) tool_call node.

    // tool_use_id -> tool_call node_id (for edges we emit later)
    let mut merged_call_node_id: HashMap<String, String> = HashMap::new();

    struct MergeAction {
        call_idx: usize,
        result_payload: Value,
        result_event_ids: Vec<String>,
        result_node_idx: usize,
        tid: String,
    }
    let mut merge_actions: Vec<MergeAction> = Vec::new();

    for (idx, n) in nodes.iter().enumerate() {
        if n.node_kind != "tool_result" {
            continue;
        }
        let tid = n
            .merge_keys
            .get("tool_use_id")
            .and_then(|x| x.as_str())
            .map(String::from);
        if let Some(tid) = tid {
            if let Some(call_idx) = tool_call_node.get(&tid).copied() {
                merge_actions.push(MergeAction {
                    call_idx,
                    result_payload: n.payload.clone(),
                    result_event_ids: n.source_event_ids.clone(),
                    result_node_idx: idx,
                    tid,
                });
            }
        }
    }

    let mut to_remove: Vec<usize> = Vec::new();
    for action in merge_actions {
        let call_nid = nodes[action.call_idx].node_id.clone();
        let mut call_payload = nodes[action.call_idx].payload.clone();
        if !call_payload.is_object() {
            call_payload = json!({});
        }
        call_payload
            .as_object_mut()
            .unwrap()
            .insert("result".into(), action.result_payload);
        nodes[action.call_idx].payload = call_payload;
        nodes[action.call_idx]
            .source_event_ids
            .extend(action.result_event_ids);

        merged_call_node_id.insert(action.tid.clone(), call_nid.clone());

        // Re-point the result node's event_uuid to the call node so that
        // downstream events with parent_uuid == result_uuid find the call node.
        if let Some((Some(result_uuid), _)) = tool_result_info.get(&action.tid) {
            by_event_uuid.insert(result_uuid.clone(), call_nid);
        }

        to_remove.push(action.result_node_idx);
    }
    // Remove in reverse so indices stay valid
    to_remove.sort_unstable();
    for idx in to_remove.into_iter().rev() {
        nodes.remove(idx);
    }

    // Build a set of valid (surviving) node_ids for edge validation
    let valid_nodes: HashSet<&str> = nodes.iter().map(|n| n.node_id.as_str()).collect();

    // 3. Edges
    let mut edges: Vec<GraphEdge> = Vec::new();

    // 3a. message_reply via parent_uuid
    for e in events {
        let Some(child_uuid) = &e.event_uuid else {
            continue;
        };
        let Some(parent_uuid) = &e.parent_uuid else {
            continue;
        };
        let (Some(child), Some(parent)) = (
            by_event_uuid.get(child_uuid),
            by_event_uuid.get(parent_uuid),
        ) else {
            continue;
        };
        // Skip self-loops or edges involving removed nodes
        if child == parent {
            continue;
        }
        if !valid_nodes.contains(child.as_str()) || !valid_nodes.contains(parent.as_str()) {
            continue;
        }
        // Deduplicate: same (parent, child) pair may appear multiple times when an
        // assistant turn emits both text and tool_use with the same event_uuid.
        let edge = make_edge(
            session_id,
            parent,
            child,
            "message_reply",
            if e.is_sidechain {
                json!({"crosses_sidechain": true})
            } else {
                json!({})
            },
        );
        if !edges.iter().any(|ex| ex.edge_id == edge.edge_id) {
            edges.push(edge);
        }
    }

    // 3b. tool_call_to_result — emitted for EVERY matched tool_call (Option A).
    //     For merged cases (result folded into call node), this is a self-loop with
    //     attribute merged=true.  For dangling tool_result nodes the call and result
    //     are distinct nodes.
    for n in &nodes {
        if n.node_kind == "tool_result" {
            // Dangling: no matching tool_call was found (or call not in events)
            let tid = n
                .merge_keys
                .get("tool_use_id")
                .and_then(|x| x.as_str())
                .map(String::from);
            if let Some(tid) = tid {
                if let Some(call_node) = nodes.iter().find(|m| {
                    m.node_kind == "tool_call"
                        && m.merge_keys.get("tool_use_id").and_then(|x| x.as_str()) == Some(&tid)
                }) {
                    edges.push(make_edge(
                        session_id,
                        &call_node.node_id.clone(),
                        &n.node_id,
                        "tool_call_to_result",
                        json!({"matched_via": "tool_use_id"}),
                    ));
                }
            }
        }
    }
    // For merged tool_calls (result was folded in), emit a self-loop edge.
    for (tid, call_nid) in &merged_call_node_id {
        if valid_nodes.contains(call_nid.as_str()) {
            edges.push(make_edge(
                session_id,
                call_nid,
                call_nid,
                "tool_call_to_result",
                json!({"matched_via": "tool_use_id", "merged": true, "tool_use_id": tid}),
            ));
        }
    }

    // 3c. turn_order — adjacent pairs of nodes ordered by (started_at, node_id).
    //     otel_span nodes are excluded: they are not conversation turns, and
    //     cross-kind turn_order edges will be wired in a later slice once
    //     transcript ↔ span correlation is established.
    let mut ordered: Vec<&GraphNode> = nodes
        .iter()
        .filter(|n| {
            !matches!(
                n.node_kind.as_str(),
                "otel_span"
                    | "file_event"
                    | "git_commit"
                    | "diff_hunk"
                    | "metric_sample"
                    | "log_record"
            )
        })
        .collect();
    ordered.sort_by(|a, b| (a.started_at, &a.node_id).cmp(&(b.started_at, &b.node_id)));
    for w in ordered.windows(2) {
        edges.push(make_edge(
            session_id,
            &w[0].node_id,
            &w[1].node_id,
            "turn_order",
            json!({}),
        ));
    }

    // Stable output ordering
    nodes.sort_by(|a, b| (a.started_at, &a.node_id).cmp(&(b.started_at, &b.node_id)));
    edges.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
    (nodes, edges)
}

fn make_edge(session_id: &str, from: &str, to: &str, kind: &str, attrs: Value) -> GraphEdge {
    GraphEdge {
        edge_id: derive_edge_id(from, to, kind),
        schema_version: SCHEMA_VERSION.into(),
        session_id: session_id.into(),
        from_node_id: from.into(),
        to_node_id: to.into(),
        edge_kind: kind.into(),
        origin: "deterministic".into(),
        attributes: attrs,
    }
}

fn canonical_pairs(v: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(map) = v.as_object() {
        for (k, vv) in map {
            out.push((k.clone(), value_to_string(vv)));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
    }
    out
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
