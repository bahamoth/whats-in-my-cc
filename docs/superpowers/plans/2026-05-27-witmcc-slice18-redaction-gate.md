# Slice-18 Implementation Plan — Redaction Gate + Manifest Emission

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice18-redaction-gate-design.md`
**Branch:** `slice18-redaction-gate`

---

## Phase 0 — Branch & baseline

| 0a | Cut `slice18-redaction-gate` off slice-17 merge |
| 0b | Record cargo + vitest baselines |

---

## Phase 1 — Red-locking tests

### Task 1 — Rule pack invariant

**Files:** `tests/redaction_rule_pack.rs`.

```rust
use witmcc::security::redaction::rules::{RULE_IDS, all_rules};

#[test]
fn rule_ids_are_canonical_and_versioned() {
    let expected = &[
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
    assert_eq!(RULE_IDS, expected);
}

#[test]
fn every_rule_compiles() {
    for r in all_rules() {
        let _ = r.compiled_regex();   // panics if invalid
    }
}
```

### Task 2 — Per-rule masking unit tests

**Files:** `tests/redaction_masking.rs`.

```rust
use witmcc::security::redaction::engine::apply_text;

#[test]
fn masks_anthropic_key_with_length_preservation() {
    let input = "key=sk-ant-api03-aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let masked = apply_text(input);
    assert!(masked.contains("sk-ant-api03-"));
    assert!(!masked.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    // length preserved
    assert_eq!(masked.len(), input.len());
}

#[test]
fn masks_private_key_block_inline() {
    let input = "header\n-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----\nfooter";
    let masked = apply_text(input);
    assert!(masked.contains("<private-key-redacted>"));
    assert!(!masked.contains("ABC"));
}

#[test]
fn masks_email_keeping_domain_and_first_char() {
    let masked = apply_text("alice@acme.com");
    assert_eq!(masked, "a***@acme.com");
}

#[test]
fn does_not_touch_safe_text() {
    let masked = apply_text("regular log line");
    assert_eq!(masked, "regular log line");
}
```

(Repeat per rule.)

### Task 3 — Manifest shape

**Files:** `tests/redaction_manifest_shape.rs`.

```rust
#[test]
fn manifest_records_rules_applied_and_counts() {
    let r = witmcc::security::redaction::engine::scan(
        "alice@acme.com and Bearer abc-def-12345-67890-zzzz1"
    );
    assert!(r.applied);
    let m = r.manifest;
    assert!(m.rules_applied.iter().any(|s| s == "email.v1"));
    assert!(m.rules_applied.iter().any(|s| s == "bearer_token.v1"));
    assert!(m.items_redacted_count >= 2);
    assert_eq!(m.redaction_state, witmcc::security::redaction::manifest::RedactionState::Redacted);
}
```

### Task 4 — Schema invariant

**Files:** `tests/migration_redaction_manifest_column.rs`.

```rust
#[tokio::test]
async fn migration_adds_redaction_manifest_column() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('raw_event')"
    ).fetch_all(&pool).await.unwrap();
    assert!(cols.iter().any(|c| c == "redaction_manifest"));
}
```

### Task 5 — Ingest wiring

**Files:** `tests/ingest_applies_redaction.rs`.

```rust
#[tokio::test]
async fn ingest_masks_secret_in_stored_payload() {
    let pool = test_pool().await;
    let raw = witmcc::model::raw::RawEventRow {
        payload_text: "key=sk-ant-api03-xxxxxxxxxxxxxxxxxxxxxxxxxxxx".into(),
        ..default_raw()
    };
    witmcc::ingest::store::store_raw_event(&pool, raw).await.unwrap();
    let stored: (String, Option<String>) = sqlx::query_as(
        "SELECT payload, redaction_manifest FROM raw_event LIMIT 1"
    ).fetch_one(&pool).await.unwrap();
    assert!(!stored.0.contains("xxxxxxxxxxxxxxxxxxxxxxxxxxxx"));
    assert!(stored.1.is_some());
}
```

### Task 6 — Pull API envelope

**Files:** `tests/api_redaction_summary.rs`.

```rust
#[tokio::test]
async fn pull_api_response_includes_redaction_summary() {
    let pool = pool_with_one_redacted_session().await;
    let server = axum_test::TestServer::new(witmcc::api::build_router(pool)).unwrap();
    let r = server.get("/v1/sessions/sess_t1/events").await;
    let b: serde_json::Value = r.json();
    assert_eq!(b["meta"]["redaction_policy"]["applied"], true);
    assert!(b["meta"]["redaction_summary"]["total_items_redacted"].as_u64().unwrap() >= 1);
}
```

### Task 7 — MCP annotation

**Files:** `tests/mcp_resources_read_redaction.rs`.

```rust
#[tokio::test]
async fn resources_read_carries_redaction_annotation() {
    let (server, sid, _) = init_with_seeded_redacted_session().await;
    let r = server.post("/mcp")
        .add_header("Mcp-Session-Id", &sid)
        .json(&serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params":{"uri":"whats-in-my-cc://sessions/sess_t1/graph"}
        }))
        .await;
    let b: serde_json::Value = r.json();
    let ann = &b["result"]["contents"][0]["annotations"];
    assert_eq!(ann["redaction_policy"]["applied"], true);
}
```

### Task 8 — Insight projection lock conversion

**Files:** `tests/redaction_shim_lock.rs` (updated).

```rust
// Replaces the slice-16 no-op assertion
#[test]
fn redaction_engine_masks_in_projection_path() {
    let projected = witmcc::insight::redaction::apply_text(
        "alice@acme.com triggered rm -rf"
    );
    assert!(!projected.contains("alice@acme.com"));
}
```

### Task 9 — Synthetic-secrets fixture

**Files:** `tests/fixtures/redaction/synthetic_secrets.jsonl`, `tests/redaction_synthetic_fixture.rs`.

```rust
#[tokio::test]
async fn synthetic_fixture_is_redacted_at_ingest() {
    let pool = test_pool().await;
    ingest_jsonl(&pool, "tests/fixtures/redaction/synthetic_secrets.jsonl").await;
    let rows: Vec<(String,)> = sqlx::query_as("SELECT payload FROM raw_event")
        .fetch_all(&pool).await.unwrap();
    for (p,) in &rows {
        assert!(!p.contains("sk-ant-api03-XXXXXXXX_REAL_LOOKING"));
        assert!(!p.contains("-----BEGIN RSA PRIVATE KEY-----"));
    }
}
```

**Commit 1:** `test(slice-18): red-locking tests for redaction rules, masking, ingest, response wiring`

---

## Phase 2 — Engine + rule pack

| 10 | `src/security/mod.rs` + `src/security/redaction/{mod,rules,engine,manifest}.rs` |
| 11 | Implement each rule per spec §2 |
| 12 | Implement `scan(text) -> ScanResult` returning masked text + manifest |
| 13 | Update `src/lib.rs` to expose `pub mod security;` |

Rule-pack + masking + manifest-shape tests green.

**Commit 2:** `feat(security): redaction rule pack v1 + engine + manifest`

---

## Phase 3 — Migration + raw_event column

| 14 | `migrations/20260601120000_0011_redaction_manifest.sql` — `ALTER TABLE raw_event ADD COLUMN redaction_manifest TEXT;` | schema test green |
| 15 | Update `RawEventRow` struct + `repo_raw.rs` insert/select to carry the new column | n/a |

**Commit 3:** `feat(db): 0011_redaction_manifest column on raw_event`

---

## Phase 4 — Ingest wiring

| 16 | `src/ingest/store.rs::store_raw_event` runs `scan()` before insert per spec §5 | ingest test green |
| 17 | Confirm all ingest paths (transcript, OTel, hook) go through `store_raw_event` (they already do) | n/a |

**Commit 4:** `feat(ingest): apply redaction gate on every raw_event write`

---

## Phase 5 — Response wiring

| 18 | Extend `ResponseMeta` per spec §6 |
| 19 | Add `aggregate_redaction_summary(pool, raw_event_ids) -> RedactionSummary` helper |
| 20 | Wire it into every endpoint handler that includes raw payload data (`events`, `findings`, `findings/.../evidence`, `verification-runs`) |
| 21 | Extend MCP `resources/read` handler to attach annotations |

Pull API + MCP redaction tests green.

**Commit 5:** `feat(api): redaction_policy + redaction_summary in envelope + MCP annotations`

---

## Phase 6 — Insight shim replacement

| 22 | Delete tracing warn from `src/insight/redaction_shim.rs` |
| 23 | Replace `redaction_shim.rs` content with `pub use crate::security::redaction::engine::apply_text;` |
| 24 | Update `tests/redaction_shim_lock.rs` per spec §8 |

Updated lock test green.

**Commit 6:** `feat(insight): replace redaction_shim with real gate`

---

## Phase 7 — Smoke + verification

```
Smoke — slice-18

[ ] witmcc init-db; ingest tests/fixtures/redaction/synthetic_secrets.jsonl
[ ] witmcc serve --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/sessions | jq '.meta.redaction_policy'
    # Expected: { applied: true, level: "standard" }
[ ] curl -s http://127.0.0.1:4337/v1/sessions/<sid>/events | jq '.meta.redaction_summary.total_items_redacted'
    # Expected: ≥ 1
[ ] curl -s http://127.0.0.1:4337/v1/sessions/<sid>/events | jq '.data.events[].payload' | grep -E '(sk-ant-api|BEGIN RSA|alice@acme)' || echo "redacted ✓"
[ ] Manual real-transcript smoke: ingest aac68973
    - curl pulled events; scan output for any obvious unredacted secrets
    - record finding in commit body
```

```
Verification — slice-18

- cargo test count: baseline (post slice-17) → expected + 14..20
- aac68973 raw_event rows: redaction_state = "redacted" or "not_redacted" — never "unredacted"
- AC-7 closed
```

---

## Phase 8 — PR

Title: `feat(slice-18): redaction gate v1 + manifest emission`. Implementation-notes update.
