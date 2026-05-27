//! Slice-18 — Rule pack invariant lock.
//!
//! Locks exactly 11 rule IDs at v1. Any addition requires a `_v2` bump.
//! Any removal is a breaking change — update the golden + note deviation.

use witmcc::security::redaction::rules::{all_rules, RULE_IDS};

#[test]
fn rule_ids_are_canonical_and_versioned() {
    let expected: &[&str] = &[
        "api_key_anthropic.v1",
        "api_key_openai.v1",
        "github_pat.v1",
        "aws_access_key_id.v1",
        "aws_secret_access_key.v1",
        "bearer_token.v1",
        "private_key_pem.v1",
        "email.v1",
        "phone.v1",
        "korean_rrn.v1",
        "high_entropy_heuristic.v1",
    ];
    assert_eq!(
        RULE_IDS, expected,
        "rule pack IDs changed — either bump to v2 or update the golden"
    );
}

#[test]
fn rule_pack_has_exactly_eleven_rules() {
    assert_eq!(
        all_rules().len(),
        11,
        "rule pack must have exactly 11 rules; adding a rule requires _v2 bump"
    );
}

#[test]
fn every_rule_compiles() {
    for r in all_rules() {
        // compiled_regex() panics on invalid pattern — boot-time guard.
        let _ = r.compiled_regex();
    }
}

#[test]
fn rule_ids_match_all_rules_order() {
    let rules = all_rules();
    let ids: Vec<&str> = rules.iter().map(|r| r.id).collect();
    assert_eq!(
        ids, RULE_IDS,
        "RULE_IDS constant and all_rules() order must be in sync"
    );
}
