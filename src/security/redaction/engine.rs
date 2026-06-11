//! Slice-18 — Redaction engine.
//!
//! `scan(text) -> ScanResult` applies rule_pack@v1 to a text payload.
//! `apply_text(text) -> String` is the convenience wrapper for the insight path.

use regex::Regex;
use std::collections::BTreeSet;

use super::manifest::{RedactionManifest, RedactionState, MANIFEST_SCHEMA_VERSION, RULE_PACK_ID};
use super::rules::{all_rules, MaskKind};

/// Result of running the redaction gate on one text payload.
pub struct ScanResult {
    /// True if any specific rule (not just heuristic) fired.
    pub applied: bool,
    /// The masked text (identical to input if nothing was masked).
    pub masked_text: String,
    /// Per-event manifest for storage.
    pub manifest: RedactionManifest,
}

/// Apply redaction gate and return a `ScanResult`.
pub fn scan(text: &str) -> ScanResult {
    if text.is_empty() {
        return ScanResult {
            applied: false,
            masked_text: text.to_string(),
            manifest: RedactionManifest::not_applicable(),
        };
    }

    let mut current = text.to_string();
    let mut rules_applied: BTreeSet<String> = BTreeSet::new();
    let mut items_count: u32 = 0;
    let mut high_entropy_only = false;

    for rule in all_rules() {
        let re = rule.compiled_regex();

        match rule.mask_kind {
            MaskKind::HeuristicFlagOnly => {
                // DEV-S18-03: high-entropy heuristic sets flag only, does NOT mask.
                // Check whether any match is NOT already covered by a specific rule.
                if re.is_match(&current) && rules_applied.is_empty() {
                    high_entropy_only = true;
                }
            }

            MaskKind::PrefixStar { prefix_len } => {
                let replaced = replace_all_length_preserving(&current, re, prefix_len);
                if replaced != current {
                    rules_applied.insert(rule.id.to_string());
                    items_count += count_matches(re, &current);
                    current = replaced;
                }
            }

            MaskKind::Email => {
                let replaced = replace_all_email(&current, re);
                if replaced != current {
                    rules_applied.insert(rule.id.to_string());
                    items_count += count_matches(re, &current);
                    current = replaced;
                }
            }

            MaskKind::Placeholder(placeholder) => {
                let replaced = re.replace_all(&current, placeholder).into_owned();
                if replaced != current {
                    rules_applied.insert(rule.id.to_string());
                    items_count += count_matches(re, &current);
                    current = replaced;
                }
            }

            MaskKind::PhoneMask => {
                let replaced = replace_all_phone(&current, re);
                if replaced != current {
                    rules_applied.insert(rule.id.to_string());
                    items_count += count_matches(re, &current);
                    current = replaced;
                }
            }

            MaskKind::StarAll => {
                let replaced = replace_all_star(&current, re);
                if replaced != current {
                    rules_applied.insert(rule.id.to_string());
                    items_count += count_matches(re, &current);
                    current = replaced;
                }
            }
        }
    }

    let applied = !rules_applied.is_empty();
    // High-entropy flag only matters when no specific rule fired.
    let has_unredacted = high_entropy_only && !applied;

    let redaction_state = if applied {
        RedactionState::Redacted
    } else {
        RedactionState::NotRedacted
    };

    let manifest = RedactionManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        rule_pack: RULE_PACK_ID,
        redaction_state,
        rules_applied: rules_applied.into_iter().collect(),
        items_redacted_count: items_count,
        has_unredacted_sensitive_payload: has_unredacted,
        review_required_before_export: has_unredacted,
    };

    ScanResult {
        applied,
        masked_text: current,
        manifest,
    }
}

/// Convenience wrapper: apply redaction and return the masked text.
/// Used by `src/insight/redaction.rs` (replaces the slice-16 shim).
pub fn apply_text(text: &str) -> String {
    scan(text).masked_text
}

// ── Masking helpers ──────────────────────────────────────────────────────────

fn count_matches(re: &Regex, text: &str) -> u32 {
    re.find_iter(text).count() as u32
}

/// Replace all matches with prefix preserved + `*` for each remaining byte.
/// Length-preserving per spec §4.
fn replace_all_length_preserving(text: &str, re: &Regex, prefix_len: usize) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0usize;

    for m in re.find_iter(text) {
        result.push_str(&text[last..m.start()]);
        let matched = m.as_str();
        let keep = matched.len().min(prefix_len);
        result.push_str(&matched[..keep]);
        let star_count = matched.len() - keep;
        for _ in 0..star_count {
            result.push('*');
        }
        last = m.end();
    }
    result.push_str(&text[last..]);
    result
}

/// Replace all matches with `*` per byte (length-preserving).
fn replace_all_star(text: &str, re: &Regex) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0usize;
    for m in re.find_iter(text) {
        result.push_str(&text[last..m.start()]);
        for _ in 0..m.as_str().len() {
            result.push('*');
        }
        last = m.end();
    }
    result.push_str(&text[last..]);
    result
}

/// Email masking: keep first char of local part + `***` + `@domain`.
/// DEV-S18-06: domain is kept.
fn replace_all_email(text: &str, re: &Regex) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0usize;
    for m in re.find_iter(text) {
        result.push_str(&text[last..m.start()]);
        let matched = m.as_str();
        if let Some(at_pos) = matched.find('@') {
            let local = &matched[..at_pos];
            let domain = &matched[at_pos..]; // includes '@'
            let first = local
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default();
            result.push_str(&first);
            result.push_str("***");
            result.push_str(domain);
        } else {
            // Fallback: star all
            for _ in 0..matched.len() {
                result.push('*');
            }
        }
        last = m.end();
    }
    result.push_str(&text[last..]);
    result
}

/// Phone masking: keep country code prefix + ` *** ***` + last 4 digits.
fn replace_all_phone(text: &str, re: &Regex) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0usize;
    for m in re.find_iter(text) {
        result.push_str(&text[last..m.start()]);
        let matched = m.as_str();
        // Extract last 4 digits
        let digits: String = matched.chars().filter(|c| c.is_ascii_digit()).collect();
        let last4 = if digits.len() >= 4 {
            digits[digits.len() - 4..].to_string()
        } else {
            digits.clone()
        };
        // Keep country code (chars until first space or digit after +)
        let prefix = if matched.starts_with('+') {
            // e.g., "+1" or "+82"
            let cc: String = matched
                .chars()
                .take_while(|&c| c == '+' || c.is_ascii_digit())
                .collect();
            cc
        } else {
            String::new()
        };
        result.push_str(&prefix);
        result.push_str(" *** *** ");
        result.push_str(&last4);
        last = m.end();
    }
    result.push_str(&text[last..]);
    result
}
