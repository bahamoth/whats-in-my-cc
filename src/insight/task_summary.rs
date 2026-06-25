//! Task summary aggregator — correlate TaskCreate/TaskUpdate tool calls into
//! per-task summaries, window-aggregating the work span (in_progress→completed):
//! verb.object tag histogram · diff (±) · verification 4-outcome · token sums.
//!
//! Pure and deterministic (measurement, not judgment — `description` is passed
//! through verbatim, never interpreted). The API endpoint maps storage rows to
//! the lightweight `*Sample` inputs (resolving each TaskCreate's taskId from its
//! tool_result, and each work/diff/verif/usage row's timestamp); this module
//! does the correlation + windowing. See `docs/implementation-notes.html`.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskOpKind {
    Create,
    Update,
}

/// A TaskCreate (taskId resolved from its result) or TaskUpdate, with its epoch-ms timestamp.
#[derive(Debug, Clone)]
pub struct TaskOp {
    pub event_id: String,
    pub task_id: String,
    pub at_ms: i64,
    pub kind: TaskOpKind,
    pub subject: Option<String>,
    pub description: Option<String>,
    pub active_form: Option<String>,
    pub status: Option<String>,
}

/// A tool_call in the session, with its verb.object tag (None = untagged/control).
pub struct TagSample {
    pub at_ms: i64,
    pub tag: Option<String>,
}
/// A diff hunk, timestamped by its introducing event.
pub struct DiffSample {
    pub at_ms: i64,
    pub added: i64,
    pub removed: i64,
}
/// A verification run outcome: passed | failed | unknown | not_executed.
pub struct VerifSample {
    pub at_ms: i64,
    pub status: String,
}
/// An assistant-output usage_facet row.
pub struct UsageSample {
    pub at_ms: i64,
    pub input: i64,
    pub output: i64,
    pub cache_creation: i64,
    pub cache_read: i64,
}

/// One status change. Carries the event_id so the UI can jump the replay to it
/// (e.g. expanding a task jumps to its in_progress event — where work started).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskTransition {
    pub status: String,
    pub at_ms: i64,
    pub event_id: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Verif4 {
    pub passed: u32,
    pub failed: u32,
    pub unknown: u32,
    pub not_executed: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tokens {
    pub input: i64,
    pub output: i64,
    pub cache_creation: i64,
    pub cache_read: i64,
}

#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub task_id: String,
    pub subject: String,
    pub description: Option<String>,
    pub active_form: Option<String>,
    pub create_event_id: String,
    pub created_at_ms: i64,
    pub status: String,
    /// status changes including the synthetic "created" first entry.
    pub transitions: Vec<TaskTransition>,
    /// last transition − created.
    pub duration_ms: Option<i64>,
    /// completed − in_progress (the active work span). None without that span.
    pub work_duration_ms: Option<i64>,
    pub saw_in_progress: bool,
    // ── work-span window aggregations (None unless there is an in_progress span) ──
    pub activity_count: Option<u32>,
    /// verb.object → count, sorted by count desc (ties keep first-seen order).
    pub tag_histogram: Vec<(String, u32)>,
    pub lines_added: Option<i64>,
    pub lines_removed: Option<i64>,
    pub verification: Option<Verif4>,
    pub tokens: Option<Tokens>,
}

struct Building {
    task_id: String,
    subject: String,
    description: Option<String>,
    active_form: Option<String>,
    create_event_id: String,
    created_at_ms: i64,
    transitions: Vec<TaskTransition>,
}

pub fn build_task_summaries(
    ops: &[TaskOp],
    tags: &[TagSample],
    diffs: &[DiffSample],
    verifs: &[VerifSample],
    usage: &[UsageSample],
) -> Vec<TaskSummary> {
    // 1) Establish tasks from Create ops (first create per taskId wins).
    let mut order: Vec<String> = Vec::new();
    let mut by_id: BTreeMap<String, Building> = BTreeMap::new();
    for op in ops {
        if op.kind != TaskOpKind::Create || by_id.contains_key(&op.task_id) {
            continue;
        }
        order.push(op.task_id.clone());
        by_id.insert(
            op.task_id.clone(),
            Building {
                task_id: op.task_id.clone(),
                subject: op.subject.clone().unwrap_or_default(),
                description: op.description.clone(),
                active_form: op.active_form.clone(),
                create_event_id: op.event_id.clone(),
                created_at_ms: op.at_ms,
                transitions: vec![TaskTransition {
                    status: "created".to_string(),
                    at_ms: op.at_ms,
                    event_id: op.event_id.clone(),
                }],
            },
        );
    }
    // 2) Apply Update ops as transitions.
    for op in ops {
        if op.kind != TaskOpKind::Update {
            continue;
        }
        let (Some(status), Some(b)) = (op.status.as_ref(), by_id.get_mut(&op.task_id)) else {
            continue;
        };
        b.transitions.push(TaskTransition {
            status: status.clone(),
            at_ms: op.at_ms,
            event_id: op.event_id.clone(),
        });
    }

    let mut out = Vec::new();
    for id in &order {
        let b = by_id.remove(id).expect("task present");
        let mut transitions = b.transitions;
        transitions.sort_by_key(|t| t.at_ms);
        let status = transitions
            .last()
            .map(|t| t.status.clone())
            .unwrap_or_default();
        let last_at = transitions
            .last()
            .map(|t| t.at_ms)
            .unwrap_or(b.created_at_ms);
        let duration_ms = (transitions.len() > 1).then_some(last_at - b.created_at_ms);
        let saw_in_progress = transitions.iter().any(|t| t.status == "in_progress");

        // Work window = [first in_progress, last transition]. None without in_progress.
        let window = transitions
            .iter()
            .find(|t| t.status == "in_progress")
            .map(|t| (t.at_ms, last_at));

        let mut summary = TaskSummary {
            task_id: b.task_id,
            subject: b.subject,
            description: b.description,
            active_form: b.active_form,
            create_event_id: b.create_event_id,
            created_at_ms: b.created_at_ms,
            status,
            work_duration_ms: window.map(|(s, e)| e - s),
            transitions,
            duration_ms,
            saw_in_progress,
            activity_count: None,
            tag_histogram: Vec::new(),
            lines_added: None,
            lines_removed: None,
            verification: None,
            tokens: None,
        };

        if let Some((start, end)) = window {
            let in_win = |at: i64| at >= start && at <= end;

            // activity + tag histogram (insertion order preserved for tie-stable sort)
            let mut count = 0u32;
            let mut hist_order: Vec<String> = Vec::new();
            let mut hist: BTreeMap<String, u32> = BTreeMap::new();
            for t in tags.iter().filter(|t| in_win(t.at_ms)) {
                count += 1;
                if let Some(tag) = &t.tag {
                    if !hist.contains_key(tag) {
                        hist_order.push(tag.clone());
                    }
                    *hist.entry(tag.clone()).or_insert(0) += 1;
                }
            }
            let mut histogram: Vec<(String, u32)> = hist_order
                .into_iter()
                .map(|k| (k.clone(), hist[&k]))
                .collect();
            histogram.sort_by(|a, b| b.1.cmp(&a.1)); // stable: ties keep first-seen
            summary.activity_count = Some(count);
            summary.tag_histogram = histogram;

            // diff (±)
            let (mut added, mut removed) = (0i64, 0i64);
            for d in diffs.iter().filter(|d| in_win(d.at_ms)) {
                added += d.added;
                removed += d.removed;
            }
            summary.lines_added = Some(added);
            summary.lines_removed = Some(removed);

            // verification 4-outcome
            let mut v = Verif4::default();
            for r in verifs.iter().filter(|r| in_win(r.at_ms)) {
                match r.status.as_str() {
                    "passed" => v.passed += 1,
                    "failed" => v.failed += 1,
                    "unknown" => v.unknown += 1,
                    "not_executed" => v.not_executed += 1,
                    _ => {}
                }
            }
            summary.verification = Some(v);

            // tokens
            let mut tok = Tokens::default();
            for u in usage.iter().filter(|u| in_win(u.at_ms)) {
                tok.input += u.input;
                tok.output += u.output;
                tok.cache_creation += u.cache_creation;
                tok.cache_read += u.cache_read;
            }
            summary.tokens = Some(tok);
        }

        out.push(summary);
    }

    // Sort by numeric task_id ascending (fallback to string).
    out.sort_by(
        |a, b| match (a.task_id.parse::<i64>(), b.task_id.parse::<i64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => a.task_id.cmp(&b.task_id),
        },
    );
    out
}
