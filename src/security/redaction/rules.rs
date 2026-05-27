//! Slice-18 — Redaction rule pack v1.
//!
//! Exactly 11 rules. Adding a rule requires a `_v2` bump (DEV-S18-01).
//! Rule patterns are compiled on first use via `OnceCell`.

use regex::Regex;
use std::sync::OnceLock;

/// Canonical ordered list of rule IDs.
/// Locked by `tests/redaction_rule_pack.rs`.
pub const RULE_IDS: &[&str] = &[
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

/// Rule kinds affect masking strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskKind {
    /// Keep prefix chars, replace remainder with `*` to preserve length.
    PrefixStar { prefix_len: usize },
    /// Keep first local char + `***` + `@domain`.
    Email,
    /// Replace entire match with a placeholder string (length not preserved).
    Placeholder(&'static str),
    /// Heuristic: don't mask the payload, only set the flag.
    HeuristicFlagOnly,
    /// Replace the match with `*` characters (length-preserving).
    StarAll,
    /// Keep country code prefix, mask middle, keep last 4 digits.
    PhoneMask,
}

pub struct RedactionRule {
    pub id: &'static str,
    pub mask_kind: MaskKind,
    pattern: &'static str,
    cell: OnceLock<Regex>,
}

impl RedactionRule {
    const fn new(id: &'static str, pattern: &'static str, mask_kind: MaskKind) -> Self {
        Self {
            id,
            mask_kind,
            pattern,
            cell: OnceLock::new(),
        }
    }

    /// Returns the compiled regex. Panics on invalid pattern (boot-time guard).
    pub fn compiled_regex(&self) -> &Regex {
        self.cell
            .get_or_init(|| Regex::new(self.pattern).expect("invalid redaction rule regex"))
    }
}

// ── Rule definitions ────────────────────────────────────────────────────────

static RULE_ANTHROPIC: RedactionRule = RedactionRule::new(
    "api_key_anthropic.v1",
    r"sk-ant-api\d{2}-[A-Za-z0-9_\-]{20,}",
    MaskKind::PrefixStar { prefix_len: 13 }, // "sk-ant-api03-" = 13 chars
);

static RULE_OPENAI: RedactionRule = RedactionRule::new(
    "api_key_openai.v1",
    r"sk-(?:(?:proj|live|test)-)?[A-Za-z0-9]{20,}",
    MaskKind::PrefixStar { prefix_len: 3 }, // "sk-" = 3 chars
);

static RULE_GITHUB_PAT: RedactionRule = RedactionRule::new(
    "github_pat.v1",
    r"gh[pousr]_[A-Za-z0-9]{36,}",
    MaskKind::PrefixStar { prefix_len: 4 }, // "ghp_" = 4 chars
);

static RULE_AWS_ACCESS_KEY_ID: RedactionRule = RedactionRule::new(
    "aws_access_key_id.v1",
    r"AKIA[0-9A-Z]{16}",
    MaskKind::PrefixStar { prefix_len: 4 }, // "AKIA" = 4 chars
);

static RULE_AWS_SECRET: RedactionRule = RedactionRule::new(
    "aws_secret_access_key.v1",
    // Only when adjacent to "aws" keyword within 30 chars; loose match
    r"(?i)(?:aws[_\-]?secret[_\-]?access[_\-]?key\s*[=:]\s*)([A-Za-z0-9/+=]{40})",
    MaskKind::PrefixStar { prefix_len: 0 },
);

static RULE_BEARER: RedactionRule = RedactionRule::new(
    "bearer_token.v1",
    r"(?i)Bearer [A-Za-z0-9._\-]{16,}",
    MaskKind::PrefixStar { prefix_len: 7 }, // "Bearer " = 7 chars
);

static RULE_PRIVATE_KEY: RedactionRule = RedactionRule::new(
    "private_key_pem.v1",
    r"-----BEGIN [A-Z ]+PRIVATE KEY-----[\s\S]*?-----END [A-Z ]+PRIVATE KEY-----",
    MaskKind::Placeholder("<private-key-redacted>"),
);

static RULE_EMAIL: RedactionRule = RedactionRule::new(
    "email.v1",
    r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}",
    MaskKind::Email,
);

static RULE_PHONE: RedactionRule = RedactionRule::new(
    "phone.v1",
    // US/KR phone patterns: +1 xxx xxx xxxx or 010-xxxx-xxxx or +82-10-xxxx-xxxx
    r"(?:\+1\s?\d{3}\s?\d{3}\s?\d{4}|\+82[\s\-]?\d{2}[\s\-]?\d{4}[\s\-]?\d{4}|010[\-\s]?\d{4}[\-\s]?\d{4})",
    MaskKind::PhoneMask,
);

static RULE_KOREAN_RRN: RedactionRule = RedactionRule::new(
    "korean_rrn.v1",
    r"\d{6}-\d{7}",
    MaskKind::Placeholder("<rrn-redacted>"),
);

static RULE_HIGH_ENTROPY: RedactionRule = RedactionRule::new(
    "high_entropy_heuristic.v1",
    // 32+ char base64/hex-looking strings that don't look like normal words
    r"[A-Za-z0-9/+=]{32,}",
    MaskKind::HeuristicFlagOnly,
);

/// Returns all rules in canonical order (matches `RULE_IDS`).
pub fn all_rules() -> &'static [&'static RedactionRule] {
    static RULES: OnceLock<Vec<&'static RedactionRule>> = OnceLock::new();
    let v = RULES.get_or_init(|| {
        vec![
            &RULE_ANTHROPIC,
            &RULE_OPENAI,
            &RULE_GITHUB_PAT,
            &RULE_AWS_ACCESS_KEY_ID,
            &RULE_AWS_SECRET,
            &RULE_BEARER,
            &RULE_PRIVATE_KEY,
            &RULE_EMAIL,
            &RULE_PHONE,
            &RULE_KOREAN_RRN,
            &RULE_HIGH_ENTROPY,
        ]
    });
    v.as_slice()
}
