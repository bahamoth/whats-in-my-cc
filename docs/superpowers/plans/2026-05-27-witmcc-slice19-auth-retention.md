# Slice-19 Implementation Plan — Token Auth + Retention Sweep

**Spec:** `docs/superpowers/specs/2026-05-27-witmcc-slice19-auth-retention-design.md`
**Branch:** `slice19-auth-retention`

---

## Phase 0 — Branch & baseline

| 0a | Cut `slice19-auth-retention` off slice-18 merge |
| 0b | Record cargo + vitest baselines |
| 0c | Set up `WITMCC_CONFIG_DIR` tempdir helper in `tests/common/config_dir.rs` |

---

## Phase 1 — Red-locking tests

### Task 1 — Token generation + file

**Files:** `tests/auth_token.rs`.

```rust
#[tokio::test]
async fn token_is_generated_on_first_serve() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WITMCC_CONFIG_DIR", dir.path());
    let token = witmcc::security::token::ensure_token().unwrap();
    assert!(token.starts_with("witmcc_"));
    // File exists with correct perm
    let tf = dir.path().join("token");
    assert!(tf.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perm = std::fs::metadata(&tf).unwrap().permissions().mode() & 0o777;
        assert_eq!(perm, 0o600);
    }
}

#[tokio::test]
async fn token_is_reused_across_calls() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("WITMCC_CONFIG_DIR", dir.path());
    let a = witmcc::security::token::ensure_token().unwrap();
    let b = witmcc::security::token::ensure_token().unwrap();
    assert_eq!(a, b);
}

#[tokio::test]
async fn refuses_to_load_when_file_overpermissive() {
    let dir = tempfile::tempdir().unwrap();
    let tf = dir.path().join("token");
    std::fs::write(&tf, "witmcc_xxx").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tf, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    std::env::set_var("WITMCC_CONFIG_DIR", dir.path());
    let r = witmcc::security::token::load_token_or_err();
    assert!(r.is_err());
}
```

### Task 2 — Auth middleware

**Files:** `tests/auth_middleware.rs`.

```rust
#[tokio::test]
async fn rejects_request_without_bearer() {
    let (server, _) = build_auth_test_server().await;
    let r = server.get("/v1/health").await;
    r.assert_status_unauthorized();
}

#[tokio::test]
async fn accepts_request_with_correct_bearer() {
    let (server, token) = build_auth_test_server().await;
    let r = server.get("/v1/health")
        .add_header(http::header::AUTHORIZATION, &format!("Bearer {token}"))
        .await;
    r.assert_status_ok();
}

#[tokio::test]
async fn rejects_request_with_wrong_bearer() {
    let (server, _) = build_auth_test_server().await;
    let r = server.get("/v1/health")
        .add_header(http::header::AUTHORIZATION, "Bearer wrong")
        .await;
    r.assert_status_unauthorized();
}

#[tokio::test]
async fn mcp_endpoint_also_requires_token() {
    let (server, _) = build_auth_test_server().await;
    let r = server.post("/mcp")
        .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}))
        .await;
    r.assert_status_unauthorized();
}
```

### Task 3 — Retention sweep

**Files:** `tests/retention_sweep.rs`.

```rust
#[tokio::test]
async fn sweep_default_profile_deletes_raw_older_than_30d() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, days_old: 31).await;
    seed_old_raw_event(&pool, days_old: 5).await;
    let p = witmcc::security::retention::RetentionPolicy { profile: Profile::Default };
    let report = witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();
    assert_eq!(report.deletions["raw_event"], 1);
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM raw_event").fetch_one(&pool).await.unwrap();
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn sweep_none_profile_deletes_nothing() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 365).await;
    let p = witmcc::security::retention::RetentionPolicy { profile: Profile::None };
    let report = witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();
    assert_eq!(report.deletions.values().sum::<u64>(), 0);
}

#[tokio::test]
async fn deleted_resource_returns_410_via_tombstone() {
    let pool = test_pool().await;
    let id = seed_old_raw_event(&pool, 31).await;
    let p = witmcc::security::retention::RetentionPolicy { profile: Profile::Default };
    witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();
    let tomb: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM retention_tombstone WHERE resource_id=?"
    ).bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(tomb, 1);
    // API would 410; covered in tests/api_resource_gone.rs (separate)
}

#[tokio::test]
async fn sweep_writes_audit_row() {
    let pool = test_pool().await;
    seed_old_raw_event(&pool, 31).await;
    let p = witmcc::security::retention::RetentionPolicy { profile: Profile::Default };
    witmcc::security::retention::run_sweep(&pool, &p).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit WHERE event='retention.deleted'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}
```

### Task 4 — 410 Gone behaviour

**Files:** `tests/api_resource_gone.rs`.

```rust
#[tokio::test]
async fn pull_api_returns_410_for_tombstoned_finding() {
    let (server, pool, token) = build_with_seed().await;
    sqlx::query("INSERT INTO retention_tombstone (resource_id, resource_kind) VALUES ('find_demo','finding')")
        .execute(&pool).await.unwrap();
    let r = server.get("/v1/findings/find_demo").add_header(http::header::AUTHORIZATION, &format!("Bearer {token}")).await;
    r.assert_status(http::StatusCode::GONE);
}
```

### Task 5 — Audit endpoint

**Files:** `tests/api_audit.rs`.

```rust
#[tokio::test]
async fn audit_endpoint_returns_rows() {
    let (server, pool, token) = build_with_seed().await;
    sqlx::query("INSERT INTO audit (audit_id, event, actor, payload) VALUES ('aud_1','api.accessed','owner','{}')")
        .execute(&pool).await.unwrap();
    let r = server.get("/v1/audit")
        .add_header(http::header::AUTHORIZATION, &format!("Bearer {token}"))
        .await;
    r.assert_status_ok();
    let b: serde_json::Value = r.json();
    assert!(b["data"].as_array().unwrap().len() >= 1);
}
```

### Task 6 — Schema invariants

**Files:** `tests/migration_retention_schema.rs`, `tests/migration_audit_schema.rs`.

Standard shape: load migrations, pragma_table_info, assert columns.

### Task 7 — CLI flag tests

**Files:** `tests/cli_token_flags.rs`.

```rust
#[test]
fn print_token_prints_to_stderr_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("witmcc").unwrap()
        .env("WITMCC_CONFIG_DIR", dir.path())
        .args(["serve","--print-token"])
        .assert().success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains("witmcc_"));
}

#[test]
fn rotate_token_changes_token() {
    let dir = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("witmcc").unwrap()
        .env("WITMCC_CONFIG_DIR", dir.path()).args(["serve","--print-token"]).assert().success();
    let t1 = std::fs::read_to_string(dir.path().join("token")).unwrap();
    assert_cmd::Command::cargo_bin("witmcc").unwrap()
        .env("WITMCC_CONFIG_DIR", dir.path()).args(["serve","--rotate-token"]).assert().success();
    let t2 = std::fs::read_to_string(dir.path().join("token")).unwrap();
    assert_ne!(t1, t2);
}
```

**Commit 1:** `test(slice-19): red-locking tests for token auth + retention sweep + audit + CLI flags`

---

## Phase 2 — Token module + middleware

| 8  | `src/security/token.rs` per spec §3: ensure_token, load_token_or_err, rotate_token |
| 9  | Wire `--print-token` and `--rotate-token` into `src/cli.rs` and `src/main.rs` (short-circuit before serve loop) |
| 10 | `src/api/middleware/auth.rs::require_token` per spec §3 |

**Commit 2:** `feat(security): token generation + persistence + middleware`

---

## Phase 3 — Apply middleware to all routes

| 11 | Wrap the `/v1/*` router with `require_token` layer |
| 12 | Wrap the `/mcp` router with the same |
| 13 | Update existing API tests to attach the token header (helper `auth_test_request_builder`) |

**Commit 3:** `feat(api): require_token on /v1 and /mcp`

---

## Phase 4 — Retention schema + repos

| 14 | `migrations/20260602120000_0012_retention.sql` (tombstone table) |
| 15 | `migrations/20260602180000_0013_audit.sql` (audit table) |
| 16 | `src/db/repo_retention.rs` (`insert_tombstone`, `is_tombstoned`, `list_tombstones`) |
| 17 | `src/db/repo_audit.rs` (`insert`, `list_recent`) |

**Commit 4:** `feat(db): 0012_retention + 0013_audit migrations + repos`

---

## Phase 5 — Sweep task + endpoint + 410 wiring

| 18 | `src/security/retention.rs` per spec §4 |
| 19 | Wire sweep task spawn into `src/main.rs::serve` (skipped when profile = none) |
| 20 | Pull API: extend `finding_detail` (and other resource-id endpoints) to check tombstones; return 410 if tomb |
| 21 | MCP: extend `resources/read` to return `isError: true, error: { code: "gone" }` |
| 22 | `src/api/routes.rs::list_audit` handler — read recent audit rows |
| 23 | Register `/v1/audit` route |
| 24 | `/v1/health` extension per spec §5 |

**Commit 5:** `feat(security): retention sweep + 410 Gone + /v1/audit + /v1/health.security`

---

## Phase 6 — Cleanup + CLAUDE.md MVP-exit pointer

| 25 | Update `docs/implementation-notes.html` Overview (slice-19) section finalising the MVP exit |
| 26 | Update CLAUDE.md `Status` block: M3..M7 all closed |
| 27 | Smoke + final MVP-exit checklist run |

**Commit 6:** `docs(slice-19): implementation-notes + CLAUDE.md MVP-exit status`

---

## Phase 7 — Smoke + verification

```
Smoke — slice-19 (basic)

[ ] rm -rf ~/.config/witmcc/token  (clean slate)
[ ] witmcc serve --print-token
    # Expected: prints "witmcc_..." to stderr and exits 0; ~/.config/witmcc/token created at mode 0600
[ ] witmcc serve --port 4337 &
[ ] curl -s http://127.0.0.1:4337/v1/health
    # Expected: 401
[ ] T=$(cat ~/.config/witmcc/token)
[ ] curl -s -H "Authorization: Bearer $T" http://127.0.0.1:4337/v1/health
    # Expected: 200 with security block
[ ] curl -s -H "Authorization: Bearer wrong" http://127.0.0.1:4337/v1/health
    # Expected: 401
[ ] (MCP Inspector smoke) connect with the bearer token in connection headers; should work.
[ ] (Rotation) witmcc serve --rotate-token; old token now 401
```

```
Smoke — slice-19 (retention)

[ ] witmcc init-db
[ ] (Synthetic) insert a raw_event with captured_at = '2020-01-01' via sqlite3 directly
[ ] witmcc serve --retention-profile default --port 4337 &
[ ] (wait 6h or call run_sweep test path); inspect /v1/audit for retention.deleted row
[ ] curl -s -H "Authorization: Bearer $T" /v1/findings/<retired_id>
    # Expected: 410
```

```
MVP-exit Smoke (full §4.4 checklist from the roadmap UX doc)

[ ] curl /v1/sessions/aac68973/episodes | jq length
[ ] curl /v1/sessions/aac68973/verification-runs | jq length
[ ] curl /v1/sessions/aac68973/graph | jq '.data.edges | map(select(.inference_rule_id != null)) | length'
[ ] curl /v1/sessions/aac68973/findings | jq length
[ ] curl /v1/findings/<id>/evidence | jq
[ ] curl /v1/findings | jq 'map(select(.provenance.judge != null)) | length'
[ ] mcp inspector connect ws://127.0.0.1:4337/mcp
[ ] curl /v1/sessions/aac68973/events | jq '.data.events[0].redaction_manifest'
[ ] curl -H "Authorization: Bearer $T" /v1/health
[ ] All ten boxes pass ⇒ MVP exited
```

```
Verification — slice-19

- cargo test count: baseline (post slice-18) → expected + 20..25
- AC-6 closed
- AC-7 closed (slice-18) + slice-19 audit complements
- All AC items 1–7 are now green
```

---

## Phase 8 — PR

Title: `feat(slice-19): token auth + retention sweep + audit (MVP exit)`. Implementation-notes update finalising the MVP. CLAUDE.md status block: `MVP closed: slice-1~19 merged`.
