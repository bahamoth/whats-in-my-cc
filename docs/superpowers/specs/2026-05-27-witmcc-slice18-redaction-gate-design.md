# Slice-18 Design — Redaction Gate + Manifest Emission

**Date:** 2026-05-27
**Branch (to be cut):** `slice18-redaction-gate` off slice-17 merge.
**Goal:** Populate the `raw_event.redaction_state` column that has lived as a placeholder since slice-1 with a real rule pack. Emit a `redaction_manifest` on every Pull API response and every MCP resource read whose underlying data contains raw payload. Replace slice-16's `redaction_shim` with the real gate.

This closes AC-7 (export manifest reports redaction_state and blocked sensitive payloads).

---

## 1. Motivation

The security spec (`docs/05_security_governance_spec.html` §4) requires:

- Secret/credential/PII/path scanning at ingest time.
- A per-payload `redaction_manifest` recording which rules fired and how many items were masked.
- A `has_unredacted_sensitive_payload` boolean used by export and preview workflows.

Today the column exists but contains a placeholder (`"unredacted"`). Insight projections that ship to the LLM judge in slice-16 carry the no-op shim's output. We close both gaps in one slice so the data plane consistently presents redacted text downstream of ingest.

---

## 2. Scope

### In scope

- New module `src/security/redaction/` with:
  - `rules.rs` — versioned rule pack (`rule_pack@v1`).
  - `engine.rs` — `apply(text, ctx) -> RedactionResult` returning the masked text + manifest.
  - `manifest.rs` — `RedactionManifest` struct + serialisation.
- Rule pack v1 covers (closed list):
  - Anthropic API key (`sk-ant-api\d{2}-[A-Za-z0-9_\-]{20,}`)
  - OpenAI API key (`sk-(proj|live|test)?[A-Za-z0-9]{20,}`)
  - GitHub PAT (`gh[pousr]_[A-Za-z0-9]{36}`)
  - AWS access key id (`AKIA[0-9A-Z]{16}`)
  - AWS secret access key (loose: `[A-Za-z0-9/+=]{40}` only when adjacent to "aws" — high-FP, requires context match)
  - Generic bearer token (`Bearer [A-Za-z0-9._\-]{16,}`)
  - Private key headers (`-----BEGIN [A-Z ]+ PRIVATE KEY-----`)
  - Email (`[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}`)
  - Phone (US/KR variants — frozen patterns)
  - Korean RRN (`\d{6}-\d{7}`)
  - Generic-looking high-entropy 32+ char base64/hex strings (last-resort heuristic, lower confidence)
- Wiring at ingest time: `src/ingest/store.rs::store_raw_event` runs the gate, sets `redaction_state`, writes a per-raw_event manifest into a new column.
- Schema change: `raw_event` gains a `redaction_manifest TEXT NULL` column (migration `0011_redaction_manifest.sql`).
- Response wiring: every Pull API endpoint that includes raw payload (envelope `data.events[*].payload`, `data.findings[*].evidence_projection`) returns the joined manifest in `meta.redaction_policy.applied = true` + a sibling `meta.redaction_summary`.
- MCP endpoint `resources/read` adds the same `redaction_summary` to the returned content's metadata field.
- Replace `src/insight/redaction_shim.rs` (from slice-16) with the real gate; the shim's tracing warn is removed.

### Out of scope

- Encryption at rest (post-MVP).
- User-configurable rule packs (rule pack is closed for MVP).
- Bidirectional unmasking (we do not store reversible mappings — masked text is one-way).
- Export bundle creation (`POST /v1/export-bundles` is a separate slice; out of MVP scope for now).

---

## 3. RedactionManifest schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionManifest {
    pub schema_version: &'static str,    // "redaction_manifest.v1"
    pub rule_pack: &'static str,         // "rule_pack@v1"
    pub redaction_state: RedactionState, // Redacted | NotRedacted | NotApplicable
    pub rules_applied: Vec<String>,      // ["api_key_anthropic.v1", ...]
    pub items_redacted_count: u32,
    pub has_unredacted_sensitive_payload: bool,
    pub review_required_before_export: bool,
}

pub enum RedactionState {
    Redacted,
    NotRedacted,            // applied gate, found nothing
    NotApplicable,          // no scannable payload
}
```

`has_unredacted_sensitive_payload` is `true` when one or more of the **high-entropy heuristic** matches fired but no specific rule did. This is the signal a future export workflow uses to require user review before allowing the export. The MVP does not have export but locks the flag now.

---

## 4. Masking conventions

- API keys: keep prefix (3-4 chars) + replace rest with `*` to the original length. Example: `sk-ant-api03-abc...xyz` → `sk-ant-api03-****************************`. Length-preserving so payload byte size stays close to original.
- Emails: keep first char of local part + domain. `alice@acme.com` → `a***@acme.com`.
- Phone: keep country code + last 4. `+1 415 555 0199` → `+1 *** *** 0199`.
- Private key block: replace **the entire block** between `-----BEGIN` and `-----END` with `<private-key-redacted>`. Length not preserved.
- Generic high-entropy: replace with `<high-entropy-string-redacted>`. Length not preserved.

A test in `tests/redaction_masking.rs` locks each rule's exact masked-shape output.

---

## 5. Ingest wiring

```rust
// src/ingest/store.rs (after slice-18)
pub async fn store_raw_event(pool: &SqlitePool, mut row: RawEventRow) -> Result<()> {
    let scan_result = redaction::engine::scan(&row.payload_text);
    if scan_result.applied {
        row.payload_text = scan_result.masked_text;
    }
    row.redaction_state = scan_result.manifest.redaction_state.as_str().to_owned();
    row.redaction_manifest = Some(serde_json::to_string(&scan_result.manifest)?);
    // INSERT row ...
}
```

The original payload is **not** retained anywhere. Masking is in-place at ingest. This is the only safe behaviour: if we kept the original somewhere, an attacker compromising the on-disk DB has the same blast radius as before. Future export workflows can require explicit user opt-in to *not* redact, but MVP does not.

---

## 6. Response wiring

### Pull API envelope

`src/model/meta.rs::ResponseMeta` gains:

```rust
#[derive(Serialize, Deserialize)]
pub struct ResponseMeta {
    pub schema_version: &'static str,
    pub request_id: String,
    pub redaction_policy: RedactionPolicy,
    pub redaction_summary: Option<RedactionSummary>,
    // ... existing fields
}

pub struct RedactionPolicy { pub applied: bool, pub level: &'static str }
pub struct RedactionSummary {
    pub total_items_redacted: u32,
    pub rules_seen: Vec<String>,
    pub any_unredacted_sensitive: bool,
}
```

Every handler computes the summary by walking the response's underlying raw event ids and aggregating their per-event manifests. The aggregation is a cheap SQL `SELECT rules_applied, items_redacted_count, has_unredacted_sensitive_payload FROM raw_event WHERE raw_event_id IN (...)`. Cached per-request.

### MCP

`resources/read` returns:

```json
{
  "result": {
    "contents": [
      {
        "uri": "...",
        "mimeType": "application/json",
        "text": "{...}",
        "annotations": {
          "redaction_policy": { "applied": true, "level": "standard" },
          "redaction_summary": { ... }
        }
      }
    ]
  }
}
```

MCP's `annotations` is the conventional carrier for non-content metadata.

---

## 7. Real-data invariant

We freeze a fixture transcript containing **known synthetic** secrets (we never freeze real ones) and assert the gate masks each:

```
tests/fixtures/redaction/synthetic_secrets.jsonl
```

Three messages:
- One containing an Anthropic key (`sk-ant-api03-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx`).
- One containing a private key PEM block.
- One containing an email + phone.

Test `tests/redaction_synthetic_fixture.rs` ingests the file, asserts the stored `raw_event.payload` does **not** contain the original secret text but the masked replacement.

Real-data anchoring is intentionally inverted for this slice: we *cannot* freeze real secrets (that would leak them into the repo). Instead, the test rigorously locks the masked output of synthetic secrets, and a manual smoke step on a real transcript ingestion verifies no obvious miss.

---

## 8. Insight projection integration

The `redaction_shim` introduced in slice-16 is replaced by:

```rust
// src/insight/redaction.rs (slice-18)
pub use crate::security::redaction::engine::apply_text;
```

The shim's tracing warn (`redaction_shim_invoked`) is removed. The shim's lock test `tests/redaction_shim_lock.rs` is **updated** (not deleted) to assert the real gate is now in place:

```rust
#[test]
fn redaction_engine_replaces_shim() {
    let masked = witmcc::insight::redaction::apply_text(
        "rm -rf /Users/alice/.ssh/id_rsa"
    );
    assert!(masked.contains("<path>") || !masked.contains("/Users/alice"));
}
```

This is an explicit conversion of a placeholder lock into a feature lock — same test file, different invariant. The commit body notes the change.

---

## 9. Failure modes

| Failure | Behaviour |
|---|---|
| Rule regex fails to compile at boot | Process fails fast. Startup test asserts every rule compiles. |
| A payload that triggers high-entropy heuristic but no specific rule | `has_unredacted_sensitive_payload = true`. Stored payload is still the original (or masked at the high-entropy substring); the flag signals review-needed. |
| Aggregating manifests for a 10 000-event session | Bounded by SQL `IN ()`. Slice limits to 200 events per query (existing pagination cap). |
| MCP client reading a resource whose underlying raw events include unredacted sensitive payload | `annotations.redaction_summary.any_unredacted_sensitive = true`. The text content still ships (it's already in the local DB; redaction is a flag, not access control). |

---

## 10. Deviations index (slice-18)

| ID | Description |
|---|---|
| DEV-S18-01 | Rule pack is closed. No runtime configuration. Adding a rule requires the slice that needs it, and a `_v2` rule-pack bump (cascade through cached judge verdicts — slice-15's cache key includes the rule pack via projection content). |
| DEV-S18-02 | Masking is **one-way**. We do not store reversible mappings, so a corrupted gate cannot leak originals. Future export-with-original workflow would need to re-ingest with a different policy. |
| DEV-S18-03 | High-entropy heuristic does **not** mask the payload — it only sets `has_unredacted_sensitive_payload = true`. False positives would otherwise destroy normal large outputs. |
| DEV-S18-04 | Real secrets are **not** in fixtures. Synthetic secrets only. Manual smoke step is part of acceptance. |
| DEV-S18-05 | `redaction_shim_lock` test is updated, not deleted. The same file becomes the regression lock for the real gate. |
| DEV-S18-06 | Email masking keeps the domain. This is a privacy / utility tradeoff — keeping the domain lets findings reference "the user's @acme.com address" without exposing identity. If feedback says this is too permissive, a `_v2` rule masks the domain too. |
| DEV-S18-07 | The Pull API envelope shape **gains** new keys in `meta`. Existing clients that parse `meta` strictly should be unaffected (`#[serde(default)]` on new fields). Tests assert the keys are present. |

---

## 11. Commit plan summary

See `2026-05-27-witmcc-slice18-redaction-gate.md`. Six commits:

1. `test(slice-18): red-locking tests for rules, masking, ingest wiring, response wiring`
2. `feat(security): redaction rule pack v1 + engine`
3. `feat(db): 0011_redaction_manifest migration + repo update`
4. `feat(ingest): wire redaction gate into store_raw_event`
5. `feat(api): redaction_policy + redaction_summary in Pull API envelope + MCP annotations`
6. `feat(insight): replace redaction_shim with real gate + update lock test`
