# Slice-19 Design — Token Auth + Retention Sweep

**Date:** 2026-05-27
**Branch (to be cut):** `slice19-auth-retention` off slice-18 merge.
**Goal:** Add bearer-token authentication to Pull API + MCP, and a background retention sweep job that prunes raw events / normalized events / findings / cache / audit per the security spec's retention table. This slice closes M7 (Hardening) and exits the MVP.

This closes AC-6 (localhost binding + token).

---

## 1. Motivation

The security spec (`docs/05_security_governance_spec.html`) lists two MVP requirements still open:

1. **Local token authentication.** Pull API + MCP must require a token even on localhost. This protects against malicious sites hitting `127.0.0.1:4337` via DNS rebinding or via a Chrome extension on a user's machine.
2. **Retention enforcement.** Raw payload retention defaults to 30 days, normalized events 180 days, audit 90 days. Without a sweep job, the local DB grows monotonically.

Both are AC-6/Definition-of-Done items; without them the MVP cannot exit honestly.

---

## 2. Scope

### In scope

- Token generation on first `witmcc serve` start. Persisted at `~/.config/witmcc/token` mode 0600. Printed once to stderr.
- New CLI flags:
  - `witmcc serve --print-token` — re-print the stored token to stderr and exit 0.
  - `witmcc serve --rotate-token` — generate a new token, overwrite the file, print the new one. Existing connections receive 401 on next request.
  - `witmcc serve --retention-profile {none|default|strict}` — choose retention. Default: `none` (no sweep). Documented in security spec but defaulted off in MVP.
- Token middleware: rejects requests missing or with wrong `Authorization: Bearer <token>` with `401`.
- Applies to both Pull API (`/v1/*`) and MCP (`/mcp`).
- `/v1/health` is **also** auth-gated (no anonymous health). The spec's localhost-bind already prevents leakage; this is defence-in-depth.
- Retention sweep:
  - `tokio` task spawned on serve start, wakes every 6 hours.
  - Each tick deletes rows older than the configured retention per class.
  - Per-class profile:

    | Profile | raw payload | normalized events | graph & insight | audit | judge_verdict_cache |
    |---|---|---|---|---|---|
    | `none` | keep | keep | keep | keep | keep |
    | `default` | 30 d | 180 d | 180 d | 90 d | 30 d (by `last_hit_at`) |
    | `strict` | 7 d | 30 d | 30 d | 30 d | 7 d |
  - Deletes write tombstones into a new `retention_tombstone` table so `resource_id`s remain resolvable (return 410 Gone).
- Audit row written on each sweep run with counts.

### Out of scope

- OAuth.
- Multi-user tokens.
- Token expiry / TTL (one durable token per installation).
- User-configurable retention policies (only the three profiles).
- Encryption at rest.

---

## 3. Token mechanics

### Generation

```rust
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    format!("witmcc_{}", base64_url_encode(&bytes))
}
```

Length: ~50 chars (43 base64url + 7-char prefix). The prefix lets a quick visual grep know it's a witmcc token, not e.g., a GitHub token.

### Storage

- Path: `~/.config/witmcc/token` (Linux/macOS XDG-style; Windows uses `%APPDATA%\witmcc\token`).
- Permissions: mode 0600 on POSIX. On Windows, the path inherits the user's ACL; we additionally clear group/others via `winapi` SetFileSecurity. Validated by a startup test that the file exists and has the expected permissions.
- On first start: if file absent, generate + write + print. If present, load.

### Middleware

```rust
// src/api/middleware/auth.rs
pub async fn require_token(req: Request, next: Next) -> Response {
    let header = req.headers().get(http::header::AUTHORIZATION);
    let expected = req.extensions().get::<AppState>().unwrap().token.as_str();
    match header.and_then(|h| h.to_str().ok()) {
        Some(s) if s == format!("Bearer {expected}") => next.run(req).await,
        _ => Response::builder().status(401).body("Unauthorized".into()).unwrap(),
    }
}
```

Constant-time comparison via `subtle::ConstantTimeEq` to avoid timing side-channels (the token is short-lived enough that this is overkill, but free).

### Rotation

`witmcc serve --rotate-token`:
1. Read existing token (or generate if absent).
2. Generate new token, atomically swap (write to `.token.tmp`, fsync, rename).
3. Print new token to stderr.
4. Continue serving with the new token; existing clients get 401 on their next request and must reload the token file.

---

## 4. Retention sweep mechanics

```rust
// src/security/retention.rs
pub struct RetentionPolicy { pub profile: Profile }

pub enum Profile { None, Default, Strict }

impl Profile {
    fn raw_payload_days(&self) -> Option<u32> { ... }
    fn normalized_event_days(&self) -> Option<u32> { ... }
    fn graph_insight_days(&self) -> Option<u32> { ... }
    fn audit_days(&self) -> Option<u32> { ... }
    fn judge_cache_days(&self) -> Option<u32> { ... }
}

pub async fn run_sweep(pool: &SqlitePool, p: &RetentionPolicy) -> Result<SweepReport> {
    // For each class, SELECT ids older than cutoff, write tombstones, DELETE rows.
    // Aggregate counts. Write one audit row "retention.deleted".
}

pub fn spawn_sweep_task(pool: SqlitePool, p: RetentionPolicy) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(6 * 60 * 60));
        loop {
            interval.tick().await;
            if let Err(e) = run_sweep(&pool, &p).await {
                tracing::warn!(?e, "retention sweep failed");
            }
        }
    })
}
```

### Tombstones

```sql
-- migrations/20260602120000_0012_retention.sql
CREATE TABLE IF NOT EXISTS retention_tombstone (
    resource_id     TEXT PRIMARY KEY,
    resource_kind   TEXT NOT NULL,        -- "raw_event" | "observed_event" | "graph_node" | "finding" | "audit" | "judge_cache"
    deleted_at      TEXT NOT NULL DEFAULT (datetime('now')),
    reason          TEXT NOT NULL DEFAULT 'retention'
);
```

On a Pull API request asking for a known-tombstoned resource id, the handler returns `410 Gone` instead of 404. This lets clients distinguish "never existed" from "expired by retention". MCP `resources/read` returns the same as an `isError: true` with `reason: "gone"`.

### Audit row

`audit` table (already in slice-1's migration footprint per security spec, though not yet built). Slice-19 ships the `audit` table too (migration `0013_audit.sql`):

```sql
CREATE TABLE IF NOT EXISTS audit (
    audit_id    TEXT PRIMARY KEY,                 -- "aud_" + ulid
    event       TEXT NOT NULL,                    -- "retention.deleted" | "api.accessed" | "mcp.connected" | "rotate.token" | ...
    actor       TEXT,                             -- "owner_or_local_client"
    payload     TEXT NOT NULL,                    -- JSON
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit(created_at);
```

Pull API exposes `GET /v1/audit` (also auth-gated). Returns recent rows up to retention.

---

## 5. Health endpoint extension

```json
{
  "status": "ok",
  "build_sha": "...",
  "insight": { ... },
  "security": {
    "auth_required": true,
    "retention_profile": "default",
    "last_sweep_at": "...",
    "last_sweep_deletions": { "raw_event": 12, "observed_event": 0, "graph_node": 0, "judge_cache": 2 }
  }
}
```

---

## 6. CLI surface

```bash
# Default: token is generated on first run; retention sweep disabled.
witmcc serve --port 4337

# Re-print the token.
witmcc serve --print-token

# Rotate.
witmcc serve --rotate-token

# Default-strength retention.
witmcc serve --retention-profile default

# Strict retention (testing / demo).
witmcc serve --retention-profile strict
```

When `--print-token` or `--rotate-token` is passed, the process does the action and exits 0 (does not start the server). This is a deliberate ergonomic choice — `witmcc print-token` would be a separate subcommand path, but the flag-on-serve keeps the CLI surface small.

---

## 7. Token-file CI/devloop interaction

- Tests in `tests/auth_token.rs` use a tempdir for the config root via `WITMCC_CONFIG_DIR` env var.
- `witmcc serve` reads `WITMCC_CONFIG_DIR` if set; defaults to `~/.config/witmcc`.
- This keeps tests hermetic.

---

## 8. Failure modes

| Failure | Behaviour |
|---|---|
| Token file is mode 0644 (overpermissive) at boot | Refuse to start; print error pointing to the file. Tested. |
| Token file is missing on `--print-token` | Generate + print as if it were a first run. |
| Retention sweep fails partway through a class | Roll back the in-progress class's transaction; other classes succeed. Audit row records partial counts. |
| Retention runs concurrent with an ingest | Both are SQLite transactions; ingest may briefly block. No deadlock (no nested transactions). |
| Tombstone refers to a finding whose evidence has also been deleted | Pull API returns 410 with no payload. Tested. |
| Clock skew makes `datetime('now') - N days` cross a DST boundary | We use UTC throughout SQLite; no DST issue. |

---

## 9. Deviations index (slice-19)

| ID | Description |
|---|---|
| DEV-S19-01 | Retention default is `none`, not `default`. The security spec says 30/180/90 should be the default; we choose to ship the *capability* with the *off* default for MVP, because deleting a user's raw payload by default is surprising. The user opts in. |
| DEV-S19-02 | `/v1/health` is auth-gated. The spec is silent on this; we choose defence-in-depth. Health probes from external monitoring tools must therefore include the token. |
| DEV-S19-03 | Token is **one per installation**, not per client. Multi-client tokens are post-MVP. |
| DEV-S19-04 | Tombstones live forever (until manually deleted). The size is small (~80 bytes / tombstone) and the cost of losing them is "user gets 404 instead of 410", which is acceptable but worse UX. |
| DEV-S19-05 | Audit table is introduced in slice-19 even though it's referenced by earlier slices' specs. Earlier slices' "audit row written" steps were aspirational; we backfill the table now. |
| DEV-S19-06 | `--rotate-token` and `--print-token` are flags on `serve` that short-circuit the server start. Cleaner subcommands could replace them later; the flag form keeps the CLI surface small. |
| DEV-S19-07 | Constant-time comparison via `subtle` is technically overkill for a local-only token, but adds zero runtime cost and silences a class of future security review concerns. |

---

## 10. Commit plan summary

See `2026-05-27-witmcc-slice19-auth-retention.md`. Six commits:

1. `test(slice-19): red-locking tests for token auth + retention sweep + audit`
2. `feat(security): token generation + persistence + file permissions`
3. `feat(api/middleware): require_token middleware on /v1 + /mcp`
4. `feat(cli): --print-token / --rotate-token / --retention-profile`
5. `feat(db): 0012_retention + 0013_audit migrations + repos`
6. `feat(security): retention sweep task + /v1/audit endpoint + /v1/health.security block`
