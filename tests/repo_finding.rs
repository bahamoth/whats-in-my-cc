//! Slice-11 — `finding` table CRUD round-trip. Tests live as a separate
//! integration test (rather than inside repo's #[cfg(test)] mod) so the
//! migration is exercised via the real sqlite migrator end-to-end.

#![cfg(test)]

use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;

use witmcc::db::{migrate, repo_finding};

#[tokio::test]
async fn insert_then_list_session() {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();

    let row = repo_finding::NewFinding {
        finding_id: "find_test_1".into(),
        schema_version: "finding.v1".into(),
        session_id: "sess_F".into(),
        category: "tool_failure".into(),
        severity: "medium".into(),
        claim: "Tool reported is_error=true".into(),
        confidence: 0.95,
        limitations: json!(["sub-classification deferred"]),
        evidence_refs: json!([{ "node_id": "nd_1", "role": "supporting" }]),
        generated_at: "2026-05-26T00:00:00Z".into(),
        rule_version: "tool_failure.v1".into(),
    };
    repo_finding::insert(&pool, &row).await.unwrap();

    let out = repo_finding::list_session(&pool, "sess_F").await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].finding_id, "find_test_1");
    assert_eq!(out[0].category, "tool_failure");
    assert_eq!(out[0].severity, "medium");
    assert!((out[0].confidence - 0.95).abs() < 1e-9);
    assert_eq!(out[0].evidence_refs[0]["node_id"], "nd_1");
}

#[tokio::test]
async fn insert_is_idempotent_on_pk_conflict() {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let row = repo_finding::NewFinding {
        finding_id: "find_dup".into(),
        schema_version: "finding.v1".into(),
        session_id: "sess_F".into(),
        category: "tool_failure".into(),
        severity: "medium".into(),
        claim: "x".into(),
        confidence: 0.9,
        limitations: json!([]),
        evidence_refs: json!([]),
        generated_at: "2026-05-26T00:00:00Z".into(),
        rule_version: "tool_failure.v1".into(),
    };
    repo_finding::insert(&pool, &row).await.unwrap();
    repo_finding::insert(&pool, &row).await.unwrap();
    let out = repo_finding::list_session(&pool, "sess_F").await.unwrap();
    assert_eq!(out.len(), 1);
}

#[tokio::test]
async fn delete_session_clears_rows() {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let row = repo_finding::NewFinding {
        finding_id: "find_del".into(),
        schema_version: "finding.v1".into(),
        session_id: "sess_DEL".into(),
        category: "tool_failure".into(),
        severity: "medium".into(),
        claim: "x".into(),
        confidence: 0.9,
        limitations: json!([]),
        evidence_refs: json!([]),
        generated_at: "2026-05-26T00:00:00Z".into(),
        rule_version: "tool_failure.v1".into(),
    };
    repo_finding::insert(&pool, &row).await.unwrap();
    repo_finding::delete_session(&pool, "sess_DEL").await.unwrap();
    let out = repo_finding::list_session(&pool, "sess_DEL").await.unwrap();
    assert!(out.is_empty());
}
