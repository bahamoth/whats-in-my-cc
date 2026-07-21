//! Task summary aggregator — correlate TaskCreate/TaskUpdate into per-task
//! summaries, window-aggregating the work span (in_progress→completed):
//! tag histogram · diff (±) · verification 4-outcome · tokens. Deterministic.
//! The endpoint maps storage rows → these lightweight samples; this is the pure core.

use wimcc::insight::task_summary::{
    build_task_summaries, DiffSample, TagSample, TaskOp, TaskOpKind, UsageSample, VerifSample,
};

fn create(event_id: &str, task_id: &str, at: i64, subject: &str, desc: &str) -> TaskOp {
    TaskOp {
        event_id: event_id.into(),
        task_id: task_id.into(),
        at_ms: at,
        kind: TaskOpKind::Create,
        subject: Some(subject.into()),
        description: Some(desc.into()),
        active_form: None,
        status: None,
    }
}
fn update(task_id: &str, at: i64, status: &str) -> TaskOp {
    TaskOp {
        event_id: format!("u{task_id}{at}"),
        task_id: task_id.into(),
        at_ms: at,
        kind: TaskOpKind::Update,
        subject: None,
        description: None,
        active_form: None,
        status: Some(status.into()),
    }
}
fn tag(at: i64, t: &str) -> TagSample {
    TagSample {
        at_ms: at,
        tag: Some(t.into()),
    }
}
fn verif(at: i64, s: &str) -> VerifSample {
    VerifSample {
        at_ms: at,
        status: s.into(),
    }
}

fn fixture() -> Vec<wimcc::insight::task_summary::TaskSummary> {
    let ops = vec![
        create(
            "c1",
            "1",
            1000,
            "테스트 위생",
            "dead 제거: (1) episode_gold (2) facet",
        ),
        create("c2", "2", 1100, "README", "README 갱신"),
        update("1", 2000, "in_progress"),
        update("1", 5000, "completed"),
        update("2", 5100, "completed"), // #2: no in_progress
    ];
    // work-span tags (only #1 has a window [2000,5000])
    let tags = vec![
        tag(1500, "read.code"), // before #1 in_progress → excluded
        tag(2500, "write.docs"),
        tag(2600, "write.docs"),
        tag(2700, "read.file"),
        tag(2800, "test.code"),
    ];
    let diffs = vec![
        DiffSample {
            at_ms: 2500,
            added: 20,
            removed: 60,
        },
        DiffSample {
            at_ms: 9000,
            added: 5,
            removed: 5,
        }, // outside → excluded
    ];
    let verifs = vec![verif(2800, "unknown"), verif(2900, "not_executed")];
    let usage = vec![
        UsageSample {
            at_ms: 2500,
            input: 100,
            output: 50,
            cache_creation: 10,
            cache_read: 1000,
        },
        UsageSample {
            at_ms: 2700,
            input: 0,
            output: 30,
            cache_creation: 0,
            cache_read: 500,
        },
    ];
    build_task_summaries(&ops, &tags, &diffs, &verifs, &usage)
}

#[test]
fn correlates_tasks_with_status_and_duration() {
    let s = fixture();
    assert_eq!(
        s.iter().map(|t| t.task_id.as_str()).collect::<Vec<_>>(),
        ["1", "2"]
    );
    let t1 = &s[0];
    assert_eq!(t1.subject, "테스트 위생");
    assert_eq!(t1.create_event_id, "c1");
    assert_eq!(t1.status, "completed");
    assert!(t1.saw_in_progress);
    assert_eq!(t1.work_duration_ms, Some(3000)); // 5000 - 2000
    assert_eq!(t1.duration_ms, Some(4000)); // 5000 - 1000
    assert_eq!(
        t1.transitions
            .iter()
            .map(|t| t.status.as_str())
            .collect::<Vec<_>>(),
        ["created", "in_progress", "completed"]
    );
    // transitions carry the event_id (for jump-to-work): created = create event.
    assert_eq!(t1.transitions[0].event_id, "c1");
    assert_eq!(t1.transitions[1].status, "in_progress");
}

#[test]
fn window_aggregations_cover_only_the_work_span() {
    let t1 = &fixture()[0];
    // activity = tags in [2000,5000] (the 1500 one excluded) = 4
    assert_eq!(t1.activity_count, Some(4));
    // histogram by tag value
    assert_eq!(
        t1.tag_histogram,
        vec![
            ("write.docs".to_string(), 2),
            ("read.file".to_string(), 1),
            ("test.code".to_string(), 1),
        ]
    );
    // diff: only the in-window hunk
    assert_eq!(t1.lines_added, Some(20));
    assert_eq!(t1.lines_removed, Some(60));
}

#[test]
fn verification_is_four_outcome_counts() {
    let v = fixture()[0].verification.clone().expect("verif");
    assert_eq!(
        (v.passed, v.failed, v.unknown, v.not_executed),
        (0, 0, 1, 1)
    );
}

#[test]
fn tokens_are_window_summed() {
    let tok = fixture()[0].tokens.clone().expect("tokens");
    assert_eq!(tok.input, 100);
    assert_eq!(tok.output, 80); // 50 + 30
    assert_eq!(tok.cache_creation, 10);
    assert_eq!(tok.cache_read, 1500); // 1000 + 500
}

#[test]
fn task_without_in_progress_has_no_window_aggregations() {
    let t2 = &fixture()[1];
    assert_eq!(t2.status, "completed");
    assert!(!t2.saw_in_progress);
    assert_eq!(t2.activity_count, None);
    assert!(t2.tag_histogram.is_empty());
    assert_eq!(t2.lines_added, None);
    assert!(t2.verification.is_none());
    assert!(t2.tokens.is_none());
}
